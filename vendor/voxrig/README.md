# Voxrig

Minecraft Java Edition 向けのRust製headless clientライブラリです。
既存の1.16.1（protocol 736）実装を維持し、版を選ぶ `Client` API に
1.21.11（protocol 774）の観測・限定的な操作用アダプタを追加しています。どちらもoffline-modeです。

外部のAI、planner、behavior treeなどに対する「身体」として、Minecraft protocol、状態同期、クライアント物理、構造化された観測、低レベル操作を提供します。経路探索、意味認識、行動計画、長期記憶といった頭脳は利用側へ委譲します。

```toml
[dependencies]
voxrig = { path = "../mc" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust,no_run
use voxrig::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25565));
    let bot = manager.connect(Player::offline("AgentOne")).await?;
    bot.wait_until_ready().await?;

    assert_eq!(Bot::protocol_info().protocol_version, 736);

    let blocks = bot.observe_snapshot(4).await?;
    let entities = bot.observe_entities(16.0).await;
    println!(
        "world revision={}, blocks={}, entities={}",
        blocks.revision,
        blocks.value.len(),
        entities.len()
    );

    bot.set_control(ControlState {
        forward: true,
        ..Default::default()
    }).await;
    bot.jump().await?;
    bot.clear_control().await;
    bot.disconnect().await?;
    Ok(())
}
```

対応機能、API、設計上の責任境界、実サーバーでの検証結果は[ドキュメント一覧](docs/index.md)を参照してください。

> [!IMPORTANT]
> 従来の `Bot` / `BotManager` とcrate rootの操作型は1.16.1専用です。
> 1.21.11は `Client` の領域観測・受信記録・通常のブロック使用に対応します。
> `observe_client_region` には、通常・粘着ピストンの移動中状態と階段形状を扱う
> [限定的なクライアント更新機構](docs/client-piston-reconstruction.md)があります。
> 受信状態と計算結果を分けて公開し、不足する処理・情報は明示します。
> 1.21.11のinventoryは、受信した単純スタックの観測と、survivalでの
> [メイン所持品・ホットバー間の確認付き交換](docs/survival-inventory.md)に限定して対応します。
> [静止した通常立位の接地判定と自身の受信状態](docs/survival-standing-context.md)も提供します。
> 1.21.11には[限定した通常歩行・ジャンプ](docs/survival-motion-controls.md)、
> [通常設置](docs/survival-placement.md)、[土・石の採掘](docs/survival-mining.md)があります。
> 移動後は独立観測、採掘後の継続は明示的な切断・復帰を要します。
> 両版のMicrosoft認証・online-mode暗号化は未対応です。
> [サバイバルAPIの境界整備](docs/survival-library-boundary.md)を進めています。
> 対応範囲と失敗した試行は[バージョン別の検証記録](docs/version-adapter-validation.md)を参照してください。

## 開発時の確認

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo test --doc
cargo clippy --all-targets -- -D warnings
```

## ライセンス

Voxrigは[MIT License](LICENSE)で提供します。組み込まれたregistry dataと
fixtureの出典・ライセンスは[Third-party notices](THIRD_PARTY_NOTICES.md)を参照してください。

### Retaining independent comparison fixtures

The former JSON probe examples are opt-in integration tests. The library and
its ordinary operations use native Rust results; only these fixture adapters
encode external comparison records. For example, after explicitly preparing an
owned disposable server and setting the existing `MC_PORT`/`TRACE_OUTPUT`
variables, `cargo test --offline --locked -j1 --test packet_trace_probe --
--ignored --exact retain_fixture --nocapture --test-threads=1` retains the same
packet/observation fixture. The corresponding tests are `circuit_observation_probe`,
`modern_operations_probe`, `client_recovery_probe`, and `player_observation_probe`.
They retain the original setup barriers and exclusive output creation. They are
ignored by default, never run as part of an offline conformance check, and do not
turn a saved record into permission to operate on another server. Existing
independent fixtures remain unchanged.
