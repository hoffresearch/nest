# FakeRecogna

FakeRecogna is a dataset comprised of real and fake news. The real news is not directly linked to fake news and vice-versa, which could lead to a biased classification. The news collection was performed by crawlers developed for mining pages of well-known and of great national importance agency news. The web crawlers were developed based on each analyzed webpage, where the extracted information is first separated into categories and then grouped by dates. The plurality of news on several pages and the different writing styles provide the dataset with great diversity for natural language processing analysis and machine learning algorithms.

## The Dataset

The news collection was performed by crawlers developed for mining pages of well-known and of great national importance agency news. The fake news mining was mainly focused on pages mentioned by the [Duke Reporters Lab](https://reporterslab.org/fact-checking/), which provides a list of pages that verify the veracity of news worldwide.There were 160 active fact-checking agencies in the world in 2019 and Brazil figures as a growing ecosystem with currently 9 initiatives and there were considered 6 out of the 9 pages during search with a great variation in the number of fake news extracted from each one, ending in 5,951 samples. Table 1 presents the current initiatives as well as the number of fake news collected from each source.

| Fact-Check Agency  | Web address        		    | # News |
| ------------------ | ------------------------------------ | ------ |
| Boatos.org  	     | https://boatos.org 		    | 2,605  |
| Fato ou Fake       | https://oglobo.globo.com/fato-ou-fake| 1,055  |
| E-farsas           | https://www.e-farsas.com             | 812    |
| UOL Confere        | https://noticias.uol.com.br/confere  | 582    |
| AFP Checamos       | https://checamos.afp.com/afp-brasil  | 509    |
| Projeto Comprova   | https://checamos.afp.com/afp-brasil  | 388    |
| Total              | -------------------------------------| 5,951  |


Concerning the real news, the crawlers searched portals such as [G1](https://g1.globo.com/), [UOL](https://www.uol.com.br/) and [Extra](https://extra.globo.com/), which are publicly recognized as reliable news outlets, besides the [Ministry of Health of Brazil](https://www.gov.br/saude/pt-br) home page, resulting in a collection of over 100,000 samples. From this set, there were filtered out 5,951 samples to keep the balance between classes and, thus, resulting in a dataset comprised of 11,902 samples.

## More informations

The FakeRecogna dataset is available at GitHub as a single XLSX file that contains 8 columns for the metadata, and each row stands for a sample (real or fake news), as described in Table 2.

| Columns  		   | Description 				| 
| ------------------------ | ------------------------------------------ |
| Title              	   | Title of article             		| 
| Sub-title (if available) | Brief description of news    		|
| News                     | Information about the article 		| 
| Category                 | News grouped according to your information |
| Author       	           | Publication author    			|
| Date   		   | Publication date    			|
| URL              	   | Article web address			|
| Class			   | 0 for fake news and 1 for real news	|

The collected texts are distributed into six categories in relation to their main subjects: Brazil, Entertainment, Health, Politics, Science, and World. These categories are defined based on the journal sections where the news were extracted. The distribution of news by category and its percentages are described in Table 3.

| Category	 | # News     | %      |
| -------------- | ---------- | ------ |
| Brazil  	 | 904        | 7.6    | 
| Entertainment  | 1,409      | 12.00  |
| Health         | 4,456      | 37.4   |
| Politics       | 3.951      | 33.1   |
| Science        | 602        | 5.1    |
| World   	 | 580        | 4.9    |
| Total          | 11,902     | 100.00 |


