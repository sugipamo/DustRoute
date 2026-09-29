# Contributing to Voxrig

Issueやpull requestを歓迎します。大きなAPI変更やprotocol version追加は、実装前にissueで
責務境界と互換性を相談してください。Voxrigは低レベルなheadless clientを担当し、pathfinding、
計画、AI runtimeは利用側の責務とします。

変更前後に次を実行してください。

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo test --doc
cargo clippy --all-targets -- -D warnings
cargo package --allow-dirty --list
```

packet parserを変更する場合は、正常系に加えて切断された入力、負数、過大count、深いnest、
圧縮展開上限などの敵対入力testを追加してください。外部projectからdataやfixtureを取り込む
場合は、source、version、変換方法、licenseを`THIRD_PARTY_NOTICES.md`へ記録してください。

Contributorは投稿した変更をrepositoryのMIT Licenseで配布することに同意するものとします。
