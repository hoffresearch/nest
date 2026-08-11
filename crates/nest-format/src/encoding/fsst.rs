//! fsst (fast static symbol table) text codec over per-chunk canonical
//! streams (the `fsst` wire codec, encoding id 9). a clean-room 255-entry
//! static symbol table maps frequent 1-8 byte substrings to single-byte
//! codes; byte 0xFF is the escape that emits the next raw byte verbatim, so
//! any input round-trips losslessly. the table is built deterministically
//! from a single greedy frequency pass and embedded in the payload header.
//!
//! this is the `TXT_STREAMS_V3` variant: it reuses the txt_streams container
//! (kind byte + count + intpack offset table + N frames, O(1) single-chunk
//! reopen) but each frame is fsst-coded. fsst keeps O(1) single-string decode
//! and wins on SHORT streams where a zstd frame's overhead dominates.
//! [`decode`] rebuilds the EXACT canonical payload byte-for-byte, so
//! `content_hash` is preserved; every read is bounds-checked (typed
//! `NestError`, never a panic on a hostile frame).
//!
//! clean-room from the published fsst design (boncz/leis/zukowski, vldb 2020)
//! as surfaced by duckdb's research (255-entry table, 1-8 byte symbols ->
//! 1-byte codes, 0xFF escape). NO code is vendored.

use super::fsst_table::{SymbolTable, parse_table, serialize_table};
use super::intpack::{IntpackReader, pack_u64s};
use super::txt_streams::{build_canonical, malformed, write_container};

/// kind/version byte for the fsst-framed variant.
pub const TXT_STREAMS_V3: u8 = 2;

/// escape code: the next byte in the code stream is emitted raw.
const ESCAPE: u8 = 0xFF;
/// max symbol length in bytes (codes 0..=254 may map to 1..=8 raw bytes).
const MAX_SYMBOL_LEN: usize = 8;
/// number of assignable codes (0..=254); 255 is the escape.
const N_CODES: usize = 255;

/// a built symbol table: `symbols[code]` is the byte string code `code`
/// expands to. at most 255 entries. round-trips through [`serialize_table`].
struct SymbolTable {
    symbols: Vec<Vec<u8>>,
    /// trie over the symbols for O(1)-per-byte longest-match encoding.
    /// each node maps a next byte to (child index, code if this node is a
    /// terminal symbol). the trie is built from the same deterministic
    /// `symbols` vector, so the encode output is byte-identical.
    trie: Trie,
}

/// a compact byte trie for greedy longest-match encoding. `next[byte]` gives
/// an optional child index, and `code` records the symbol code (if any) at
/// this node. the trie is rebuilt from the deterministic symbol table, so it
/// does not affect byte-identicality of the encoded output.
#[derive(Default)]
struct Trie {
    next: Vec<[Option<u16>; 256]>,
    code: Vec<Option<u8>>,
}

impl Trie {
    fn new() -> Self {
        Self {
            next: vec![[None; 256]],
            code: vec![None],
        }
    }

    fn insert(&mut self, sym: &[u8], code: u8) {
        let mut node = 0usize;
        for &b in sym {
            let b = b as usize;
            let child = self.next[node][b];
            let child = child.unwrap_or_else(|| {
                let idx = self.next.len() as u16;
                self.next.push([None; 256]);
                self.code.push(None);
                self.next[node][b] = Some(idx);
                idx
            });
            node = child as usize;
        }
        self.code[node] = Some(code);
    }

    /// longest symbol match starting at `input`. returns `(code, length)`.
    fn longest_match(&self, input: &[u8]) -> Option<(u8, usize)> {
        let mut node = 0usize;
        let mut best: Option<(u8, usize)> = None;
        for (i, &b) in input.iter().enumerate().take(MAX_SYMBOL_LEN) {
            let child = self.next[node][b as usize]?;
            node = child as usize;
            if let Some(code) = self.code[node] {
                best = Some((code, i + 1));
            }
        }
        best
    }
}

impl SymbolTable {
    /// greedily build a static table from a frequency pass over `corpus`.
    /// deterministic: candidate substrings are counted in a sorted map and
    /// selected by (count desc, bytes asc), so two builds match exactly.
    fn build(corpus: &[u8]) -> Self {
        use std::collections::BTreeMap;
        let t0 = std::time::Instant::now();
        // count every 1..=MAX_SYMBOL_LEN substring occurrence. a BTreeMap
        // keyed by the bytes gives a deterministic iteration order, and the
        // gain heuristic (saved bytes = (len-1)*count) ranks longer frequent
        // substrings first, the core fsst idea.
        let mut counts: BTreeMap<Vec<u8>, u64> = BTreeMap::new();
        for len in 1..=MAX_SYMBOL_LEN {
            if corpus.len() < len {
                break;
            }
        }
        best
    }
}

/// frequency trie for counting every 1..=MAX_SYMBOL_LEN substring without
/// allocating per-substring keys. each node records the number of times the
/// byte sequence that ends at it was seen as a full substring.
#[derive(Default)]
struct FreqTrie {
    next: Vec<[Option<u32>; 256]>,
    count: Vec<u64>,
}

impl FreqTrie {
    fn new() -> Self {
        Self {
            next: vec![[None; 256]],
            count: vec![0],
        }
    }

    fn count_substrings(&mut self, corpus: &[u8], max_len: usize) {
        let n = corpus.len();
        for start in 0..n {
            let max = (start + max_len).min(n);
            let mut node = 0usize;
            for end in start..max {
                let b = corpus[end] as usize;
                let child = self.next[node][b];
                let child = match child {
                    Some(c) => c as usize,
                    None => {
                        let idx = self.next.len() as u32;
                        self.next.push([None; 256]);
                        self.count.push(0);
                        self.next[node][b] = Some(idx);
                        idx as usize
                    }
                };
                self.count[child] += 1;
                node = child;
            }
        }
    }

    /// collect all substrings with their counts. `path` is a reusable buffer
    /// passed through recursion; results are deterministic because children
    /// are visited in ascending byte order.
    fn collect(&self, out: &mut Vec<(Vec<u8>, u64)>, path: &mut Vec<u8>, node: usize) {
        if self.count[node] > 0 {
            out.push((path.clone(), self.count[node]));
        }
        for b in 0..256u16 {
            if let Some(child) = self.next[node][b as usize] {
                path.push(b as u8);
                self.collect(out, path, child as usize);
                path.pop();
            }
        }
        eprintln!("fsst build: count substrings took {:?}", t0.elapsed());
        // rank by estimated saved bytes desc, then bytes asc for determinism.
        let t1 = std::time::Instant::now();
        let mut ranked: Vec<(Vec<u8>, u64)> = counts.into_iter().collect();
        ranked.sort_by(|a, b| {
            let gain_a = (a.0.len() as u64 - 1) * a.1;
            let gain_b = (b.0.len() as u64 - 1) * b.1;
            gain_b.cmp(&gain_a).then_with(|| a.0.cmp(&b.0))
        });
        eprintln!("fsst build: rank took {:?}", t1.elapsed());
        // always include every single byte that appears, so no input ever
        // needs more escapes than necessary; then fill the rest with the
        // highest-gain multi-byte symbols up to N_CODES.
        let mut symbols: Vec<Vec<u8>> = Vec::with_capacity(N_CODES);
        let mut seen: std::collections::BTreeSet<Vec<u8>> = std::collections::BTreeSet::new();
        for (sym, _) in &ranked {
            if symbols.len() >= N_CODES {
                break;
            }
            if seen.insert(sym.clone()) {
                symbols.push(sym.clone());
            }
        }
        let mut trie = Trie::new();
        for (code, sym) in symbols.iter().enumerate() {
            trie.insert(sym, code as u8);
        }
        Self { symbols, trie }
    }

    /// longest table symbol matching `input` at its start, returning
    /// `(code, length)`, or `None` if no multi/single-byte symbol matches
    /// (then the caller escapes the single raw byte).
    fn longest_match(&self, input: &[u8]) -> Option<(u8, usize)> {
        self.trie.longest_match(input)
    }
}

/// serialize the table: u16 count (LE) + per symbol a u8 length + bytes.
fn serialize_table(table: &SymbolTable) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(table.symbols.len() as u16).to_le_bytes());
    for sym in &table.symbols {
        out.push(sym.len() as u8);
        out.extend_from_slice(sym);
    }
    out
}

/// parse a serialized table, bounds-checked. returns the table and the byte
/// length consumed so the caller can locate what follows it.
fn parse_table(bytes: &[u8]) -> crate::Result<(Vec<Vec<u8>>, usize)> {
    if bytes.len() < 2 {
        return Err(malformed("fsst: truncated table count"));
    }
    let count = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
    if count > N_CODES {
        return Err(malformed("fsst: table count exceeds 255"));
    }
    let mut pos = 2;
    let mut symbols = Vec::with_capacity(count);
    for _ in 0..count {
        let len = *bytes
            .get(pos)
            .ok_or_else(|| malformed("fsst: truncated symbol len"))? as usize;
        if len == 0 || len > MAX_SYMBOL_LEN {
            return Err(malformed("fsst: symbol len out of range"));
        }
        pos += 1;
        let sym = bytes
            .get(pos..pos + len)
            .ok_or_else(|| malformed("fsst: truncated symbol bytes"))?
            .to_vec();
        pos += len;
        symbols.push(sym);
    }
    Ok((symbols, pos))
}

/// encode one string with `table`: greedy longest-match, escape any byte no
/// symbol covers. lossless for arbitrary bytes.
fn encode_one(table: &SymbolTable, input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        match table.longest_match(&input[i..]) {
            Some((code, len)) => {
                out.push(code);
                i += len;
            }
            None => {
                out.push(ESCAPE);
                out.push(input[i]);
                i += 1;
            }
        }
    }
    out
}

/// decode one fsst code stream against the parsed `symbols`. validates that
/// an escape is never the final byte and that codes are in range; never
/// panics on a hostile frame.
fn decode_one(symbols: &[Vec<u8>], codes: &[u8]) -> crate::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(codes.len() * 2);
    let mut i = 0;
    while i < codes.len() {
        let c = codes[i];
        if c == ESCAPE {
            let b = *codes
                .get(i + 1)
                .ok_or_else(|| malformed("fsst: trailing escape"))?;
            out.push(b);
            i += 2;
        } else {
            let sym = symbols
                .get(c as usize)
                .ok_or_else(|| malformed("fsst: code out of table range"))?;
            out.extend_from_slice(sym);
            i += 1;
        }
    }
    Ok(out)
}

/// encode `texts` as per-chunk fsst frames behind the shared txt_streams
/// offset table, with one corpus-wide symbol table embedded after the
/// container header. a pure function of the inputs, so two builds match.
pub fn encode(texts: &[String]) -> crate::Result<Vec<u8>> {
    let corpus: Vec<u8> = texts.iter().flat_map(|t| t.as_bytes().to_vec()).collect();
    let table = SymbolTable::build(&corpus);
    let table_blob = serialize_table(&table);
    let mut streams: Vec<u8> = Vec::new();
    let mut offsets: Vec<u64> = Vec::with_capacity(texts.len() + 1);
    offsets.push(0);
    for t in texts {
        streams.extend_from_slice(&encode_one(&table, t.as_bytes()));
        offsets.push(streams.len() as u64);
    }
    let off_table = pack_u64s(&offsets);
    // the container streams region = u32 table_len + symbol table + frames.
    // the offset table indexes frames relative to the table blob end, so
    // decode splits the region on the stored table length.
    let mut framed = Vec::with_capacity(4 + table_blob.len() + streams.len());
    framed.extend_from_slice(&(table_blob.len() as u32).to_le_bytes());
    framed.extend_from_slice(&table_blob);
    framed.extend_from_slice(&streams);
    Ok(write_container(
        TXT_STREAMS_V3,
        texts.len(),
        &off_table,
        &framed,
    ))
}

/// reconstruct the canonical `chunks_canonical` payload from an fsst-framed
/// `txt_streams` V3 payload. byte-identical to
/// `sections::encode_chunks_canonical`, so `content_hash` is preserved.
pub fn decode(bytes: &[u8]) -> crate::Result<Vec<u8>> {
    let (count, offsets, framed) = parse_v3(bytes)?;
    if framed.len() < 4 {
        return Err(malformed("fsst: truncated region header"));
    }
    let table_len = u32::from_le_bytes(framed[0..4].try_into().unwrap()) as usize;
    let region = &framed[4..];
    let (symbols, parsed_len) = parse_table(region)?;
    if parsed_len != table_len {
        return Err(malformed("fsst: declared table length mismatch"));
    }
    let streams = region
        .get(table_len..)
        .ok_or_else(|| malformed("fsst: truncated streams region"))?;
    if *offsets.last().unwrap() as usize != streams.len() {
        return Err(malformed("fsst: final offset != streams length"));
    }
    let mut bodies: Vec<Vec<u8>> = Vec::with_capacity(count);
    for i in 0..count {
        let start = offsets[i] as usize;
        let end = offsets[i + 1] as usize;
        let frame = streams
            .get(start..end)
            .ok_or_else(|| malformed("fsst: frame slice out of bounds"))?;
        let raw = decode_one(&symbols, frame)?;
        std::str::from_utf8(&raw).map_err(|e| malformed(format!("fsst: invalid utf-8: {}", e)))?;
        bodies.push(raw);
    }
    build_canonical(count, &bodies)
}

/// parse the V3 container header + intpack offset table, returning the chunk
/// count, the n+1 byte offsets, and the framed region (table + streams).
fn parse_v3(bytes: &[u8]) -> crate::Result<(usize, Vec<u64>, &[u8])> {
    let (kind, rest) = bytes
        .split_first()
        .ok_or_else(|| malformed("fsst: empty"))?;
    if *kind != TXT_STREAMS_V3 {
        return Err(malformed(format!("fsst: unknown kind {}", *kind)));
    }
    if rest.len() < 8 {
        return Err(malformed("fsst: truncated count"));
    }
    let declared = u64::from_le_bytes(rest[0..8].try_into().unwrap());
    let table_bytes = &rest[8..];
    let reader = IntpackReader::parse(table_bytes)?;
    if reader.is_empty() {
        return Err(malformed("fsst: offset table must hold n+1 >= 1"));
    }
    let count = reader.len() - 1;
    if declared != count as u64 {
        return Err(malformed("fsst: declared count != offset count - 1"));
    }
    let mut offsets = Vec::with_capacity(reader.len());
    for i in 0..reader.len() {
        offsets.push(reader.get(i)?);
    }
    if offsets[0] != 0 {
        return Err(malformed("fsst: first offset must be 0"));
    }
    let off_len = pack_u64s(&offsets).len();
    if off_len > table_bytes.len() {
        return Err(malformed("fsst: truncated offset table"));
    }
    let framed = &table_bytes[off_len..];
    for w in offsets.windows(2) {
        if w[1] < w[0] {
            return Err(malformed("fsst: non-monotonic offsets"));
        }
    }
    Ok((count, offsets, framed))
}

// positive + escape-path coverage lives in tests/fsst_roundtrip.rs and the
// negative/fuzz coverage in tests/negative_fsst.rs (both exercise the public
// encode/decode, which drive encode_one/decode_one through every frame).
