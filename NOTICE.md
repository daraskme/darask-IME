# Third-party material

This repository's own code is MIT-licensed (see LICENSE). It builds on, and
downloads at build time, the following data. None of it is stored in this repo
except where noted.

- **Mozc** (BSD-3-Clause, Copyright Google Inc.) — the romaji→kana table
  `crates/core/romaji.tsv` is derived from Mozc's `romanji-hiragana.tsv` and is
  included here. The dictionary (`dictionary_oss`) and connection costs are read
  from the `mozc` source at build time; they originate from IPAdic
  (NAIST, see Mozc's `src/data/dictionary_oss/README.txt` for its terms) and the
  Okinawa Dictionary. Redistributing the *built data* means complying with
  those licenses.
- **CMUdict** (BSD-style, Carnegie Mellon University) — English pronunciations,
  fetched at build time.
- **Peter Norvig's count_1w.txt** (derived from the Google Web Trillion Word
  Corpus, MIT-licensed) — English word frequencies, fetched at build time.
- **JMdict** (CC BY-SA 4.0, Electronic Dictionary Research and Development
  Group) — optional, only used by local dev builds if present; not bundled.
