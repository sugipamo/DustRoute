# 導入と最初の接続

## 必要な環境

- Rust 1.85以上
- Tokio runtime
- Minecraft Java Edition 1.16.1 server
- server設定：`online-mode=false`

offline-modeとは認証を省略したMinecraft serverへ接続するという意味であり、serverなしで動作することではありません。

## Dependency

ローカルcheckoutをpath dependencyとして参照します。

```toml
[dependencies]
voxrig = { path = "../mc" }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time"] }
```

## 1 Botを接続する

```rust,no_run
use voxrig::prelude::*;

#[tokio::main]
async fn main() -> Result<()> {
    let manager = BotManager::new(Server::new("127.0.0.1", 25565));
    let bot = manager.connect(Player::offline("AgentOne")).await?;
    bot.wait_until_ready().await?;

    println!("player={:?}", bot.player().await);
    println!("survival={:?}", bot.survival_state().await);

    bot.disconnect().await?;
    Ok(())
}
```

usernameは3〜16文字のASCII英数字またはunderscoreにします。`wait_until_ready()`が完了してから座標を必要とする操作を始めてください。

## 継続移動

```rust,ignore
bot.set_control(ControlState {
    forward: true,
    sprint: true,
    ..Default::default()
}).await;

tokio::time::sleep(std::time::Duration::from_secs(2)).await;
bot.clear_control().await;
```

`set_control()`は入力状態を設定します。一定距離や目的地までの移動は外部controllerが観測し、停止条件を決めます。

## 複数Bot

```rust,ignore
let (builder, explorer) = tokio::try_join!(
    manager.connect(Player::offline("BuilderBot")),
    manager.connect(Player::offline("ExplorerBot")),
)?;

tokio::try_join!(builder.wait_until_ready(), explorer.wait_until_ready())?;
builder.set_control(ControlState { forward: true, ..Default::default() }).await;
explorer.jump().await?;
```

各Botは独立したplayer、world、inventory、entity tracker、physics loopを持ちます。managerの購読イベントには発生元usernameが付与されます。

## 次に読む文書

- 利用可能な操作を確認する：[対応機能と制約](capabilities.md)
- 型とmethodを探す：[公開API](api.md)
- AI側との境界を確認する：[設計と責任境界](architecture.md)
