"""ffmpeg-backed media encode and decode for image corpora.

Every image is normalized onto one fixed canvas before encoding, and frames
travel in and out through a rawvideo pipe. That keeps `frame[i]` bound to
`image[i]` by construction: the concat demuxer used by the earlier draft
re-derives timestamps per input and drops frames it considers out of order,
which silently shifts every URI in the corpus. A raw pipe cannot drop a
frame without the byte count disagreeing, and `frame_count` is verified
against the image count after every encode.

Letterboxing (fit inside the canvas, pad the remainder) is used instead of
a plain rescale so a dataset with mixed aspect ratios is not distorted.
The canvas is derived from the dataset's median aspect ratio, so a
homogeneous dataset wastes no pixels on padding.
"""

from __future__ import annotations

import hashlib
import json
import subprocess
from collections.abc import Iterator, Sequence
from pathlib import Path

import numpy as np

# yuv420p subsamples chroma 2x in both axes, so both canvas dimensions must
# be even or ffmpeg silently rounds and the decoded size stops matching.
_EVEN = 2


def sha256_file(path: Path, *, chunk: int = 1 << 20) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as handle:
        while block := handle.read(chunk):
            digest.update(block)
    return "sha256:" + digest.hexdigest()


def _round_even(value: int) -> int:
    return max(_EVEN, int(value) - (int(value) % _EVEN))


def canvas_size(paths: Sequence[Path], width: int, *, probe_cap: int = 128) -> tuple[int, int]:
    """Pick one canvas for the whole dataset from its median aspect and width.

    `width` is a CEILING, not a target. Upscaling invents no detail, so every
    interpolated pixel is a bit the encoder spends on nothing and a decoder
    pays for on every read: measured on PH2, a 1024 canvas over a 765-wide
    source cost 33 percent of the file and 26 percent of the single-frame
    decode, for no information. Both the aspect and the cap come from the
    MEDIAN so one outsized image cannot set the geometry for the corpus.

    Only `probe_cap` evenly spaced images are opened: reading every file in a
    100k-image dataset costs minutes and the median is stable well before
    that. Evenly spaced (not random) keeps the choice reproducible.
    """
    from PIL import Image

    if not paths:
        raise ValueError("canvas_size needs at least one image")
    step = max(1, len(paths) // probe_cap)
    ratios, widths = [], []
    for path in list(paths)[::step][:probe_cap]:
        with Image.open(path) as img:
            w, h = img.size
        if w > 0 and h > 0:
            ratios.append(w / h)
            widths.append(w)
    if not ratios:
        raise RuntimeError("no readable images while choosing the canvas")
    aspect = float(np.median(ratios))
    target = min(int(width), int(np.median(widths)))
    return _round_even(target), _round_even(round(target / aspect))


def letterbox(image, canvas: tuple[int, int]):
    """Fit `image` inside `canvas` preserving aspect, pad the rest black."""
    from PIL import Image, ImageOps

    return ImageOps.pad(
        image.convert("RGB"), canvas, method=Image.Resampling.BICUBIC, color=(0, 0, 0)
    )


def _read_exact(stream, size: int) -> bytes:
    """Pipes return short reads; loop until `size` bytes or a clean EOF."""
    buf = bytearray()
    while len(buf) < size:
        block = stream.read(size - len(buf))
        if not block:
            break
        buf.extend(block)
    return bytes(buf)


def probe_frame_count(path: Path) -> int:
    """Count decoded frames. `nb_frames` is a container hint and is often
    absent or wrong for AV1 in mp4, so the frames are actually counted."""
    # fmt: off
    cmd = [
        "ffprobe", "-v", "error", "-select_streams", "v:0",
        "-count_frames", "-show_entries", "stream=nb_read_frames",
        "-of", "json", str(path),
    ]
    # fmt: on
    out = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        check=True,
    )
    streams = json.loads(out.stdout).get("streams") or [{}]
    return int(streams[0].get("nb_read_frames") or 0)


def encode_av1(
    image_paths: Sequence[Path],
    output_path: Path,
    *,
    canvas: tuple[int, int],
    crf: int = 35,
    preset: int = 8,
    fps: int = 1,
    lp: int = 2,
    keyint: int | None = None,
) -> dict:
    """Encode an ordered image list to one AV1 mp4 through a rawvideo pipe.

    `keyint=1` makes every frame a keyframe. That is worth trying on any
    corpus whose images are unrelated to each other: inter-frame prediction
    has nothing to find, and the bits it spends looking are wasted. On PH2
    (200 dermoscopy images) all-intra came out 16 percent SMALLER than the
    default gop, and a single-frame decode dropped from 100 ms to 74 ms
    because there is no keyframe to seek back to. Left off by default,
    since that is one dataset.

    Raises if the encoded frame count disagrees with the image count: a
    corpus whose `#frame=N` pointers are off by one is worse than no corpus.
    """
    from PIL import Image

    width, height = canvas
    source_bytes = sum(p.stat().st_size for p in image_paths)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    svt_params = f"lp={lp}" + (f":keyint={keyint}" if keyint else "")

    # fmt: off
    cmd = [
        "ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
        "-f", "rawvideo", "-pix_fmt", "rgb24",
        "-s", f"{width}x{height}", "-r", str(fps), "-i", "-",
        "-c:v", "libsvtav1", "-crf", str(crf), "-preset", str(preset),
        "-svtav1-params", svt_params,
        "-pix_fmt", "yuv420p", "-frames:v", str(len(image_paths)),
        str(output_path),
    ]
    # fmt: on
    proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stderr=subprocess.PIPE)
    try:
        for path in image_paths:
            with Image.open(path) as img:
                frame = letterbox(img, canvas)
            proc.stdin.write(np.asarray(frame, dtype=np.uint8).tobytes())
        proc.stdin.close()
        failure = "ffmpeg encode failed" if proc.wait() != 0 else None
    except BrokenPipeError:
        proc.wait()
        failure = "ffmpeg closed the stream early"
    stderr = proc.stderr.read().decode()
    proc.stderr.close()
    if failure:
        raise RuntimeError(f"{failure}: {stderr}")

    frames = probe_frame_count(output_path)
    if frames != len(image_paths):
        raise RuntimeError(
            f"encoded {frames} frames for {len(image_paths)} images; "
            "frame pointers would be misaligned"
        )

    output_bytes = output_path.stat().st_size
    return {
        "codec": "libsvtav1",
        "crf": crf,
        "preset": preset,
        "fps": fps,
        "keyint": keyint,
        "canvas": [width, height],
        "frame_count": frames,
        "source_bytes": source_bytes,
        "output_bytes": output_bytes,
        "compression_ratio": round(source_bytes / output_bytes, 2) if output_bytes else 0.0,
        "media_sha256": sha256_file(output_path),
    }


def decode_frames(
    video_path: Path, canvas: tuple[int, int], *, batch_size: int = 32
) -> Iterator[list[np.ndarray]]:
    """Yield decoded RGB frames in order, in batches.

    Sequential decode, not per-frame seeking: seeking an AV1 stream costs a
    keyframe re-decode per call and turns embedding a corpus into an O(n^2)
    walk. Batching keeps peak memory at `batch_size` frames.
    """
    width, height = canvas
    frame_bytes = width * height * 3
    # fmt: off
    cmd = [
        "ffmpeg", "-v", "error", "-i", str(video_path),
        "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
    ]
    # fmt: on
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    batch: list[np.ndarray] = []
    abandoned = True
    try:
        while True:
            raw = _read_exact(proc.stdout, frame_bytes)
            if len(raw) < frame_bytes:
                break
            batch.append(np.frombuffer(raw, dtype=np.uint8).reshape(height, width, 3))
            if len(batch) == batch_size:
                yield batch
                batch = []
        if batch:
            yield batch
        abandoned = False
    finally:
        proc.stdout.close()
        if abandoned:
            # the consumer stopped early, so ffmpeg dies on a broken pipe.
            # that is not a decode failure and must not be reported as one.
            proc.terminate()
            proc.wait()
            proc.stderr.close()
        else:
            code = proc.wait()
            stderr = proc.stderr.read().decode()
            proc.stderr.close()
            if code != 0:
                raise RuntimeError(f"ffmpeg decode failed: {stderr}")


def media_dir_for(nest_path: Path) -> Path:
    """Where a corpus keeps its media, as a sibling of the `.nest`.

    A corpus is `corpus.nest` plus `corpus.media/`. Keeping media beside the
    file (never at an absolute path baked into a URI) is what lets a corpus
    be copied to another machine and still resolve.
    """
    nest_path = Path(nest_path)
    return nest_path.parent / f"{nest_path.stem}.media"


def parse_media_uri(uri: str) -> tuple[str, int]:
    """Split `media://<name>#frame=<n>` into its file name and ordinal."""
    if not uri.startswith("media://"):
        raise ValueError(f"not a media uri: {uri}")
    body, _, fragment = uri[len("media://") :].partition("#")
    if not fragment.startswith("frame="):
        raise ValueError(f"media uri has no frame ordinal: {uri}")
    return body, int(fragment[len("frame=") :])


def decode_frame(video_path: Path, canvas: tuple[int, int], index: int) -> np.ndarray:
    """Decode a single frame by ordinal, for read-side hit resolution.

    Uses a select filter rather than walking `decode_frames`, so pulling one
    hit out of a 10k-frame corpus does not decode the 9999 frames before it.
    """
    width, height = canvas
    # fmt: off
    cmd = [
        "ffmpeg", "-v", "error", "-i", str(video_path),
        "-vf", f"select=eq(n\\,{int(index)})", "-fps_mode", "passthrough",
        "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-",
    ]
    # fmt: on
    proc = subprocess.run(cmd, capture_output=True, check=False)
    expected = width * height * 3
    if proc.returncode != 0:
        raise RuntimeError(f"ffmpeg decode failed: {proc.stderr.decode()}")
    if len(proc.stdout) < expected:
        raise IndexError(f"frame {index} not present in {video_path}")
    return np.frombuffer(proc.stdout[:expected], dtype=np.uint8).reshape(height, width, 3)
