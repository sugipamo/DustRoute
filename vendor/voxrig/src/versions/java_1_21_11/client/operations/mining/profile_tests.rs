use super::*;
use crate::checked_survival::MiningRecoveryTarget;

#[tokio::test]
async fn profile_recovery_requires_original_closed_source_before_any_login() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = crate::Server::new("127.0.0.1", listener.local_addr().unwrap().port());
    let mut miner = Fixture::new().await;
    miner
        .session
        .state
        .lock()
        .await
        .identity
        .as_mut()
        .unwrap()
        .server = endpoint.clone();
    let intent = miner.start().await;
    let source = crate::Client::from_java_1_21_11(miner.api.bot.clone())
        .survival()
        .unwrap();
    let recovery = source
        .prepare_mining_profile_recovery(&intent)
        .await
        .unwrap();
    let config = ConnectionConfig::offline(endpoint, "Miner42", MinecraftVersion::Java1_21_11);
    assert!(
        recovery
            .reconnect(config.clone(), MiningRecoveryTarget::OriginalOrAir)
            .await
            .is_err()
    );
    assert!(
        source
            .operation_history()
            .await
            .mining
            .unwrap()
            .recovery_attempt
            .is_none()
    );
    assert!(
        timeout(Duration::from_millis(10), listener.accept())
            .await
            .is_err()
    );
    let other = Fixture::new_id(43).await;
    assert!(
        other
            .api
            .prepare_survival_mining_profile_recovery(&intent)
            .await
            .is_err()
    );
    recovery.close_source().await.unwrap();
    let mut wrong = config.clone();
    wrong.username = "DifferentMiner".into();
    assert!(
        recovery
            .reconnect(wrong, MiningRecoveryTarget::OriginalOrAir)
            .await
            .is_err()
    );
    assert!(
        recovery
            .reconnect(config, MiningRecoveryTarget::Exact(native("dirt")))
            .await
            .is_err()
    );
    assert!(
        source
            .operation_history()
            .await
            .mining
            .unwrap()
            .recovery_attempt
            .is_none()
    );
    assert!(
        timeout(Duration::from_millis(10), listener.accept())
            .await
            .is_err()
    );
    other.stop().await;
    miner.stop().await;
}

#[tokio::test]
async fn profile_recovery_cancelled_login_blocks_clones_and_independent_method() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = crate::Server::new("127.0.0.1", listener.local_addr().unwrap().port());
    let mut miner = Fixture::new().await;
    let mut observer = Fixture::new_id(43).await;
    for f in [&miner, &observer] {
        f.session
            .state
            .lock()
            .await
            .identity
            .as_mut()
            .unwrap()
            .server = endpoint.clone();
    }
    let intent = miner.start().await;
    observer.profile(42).await;
    let source = crate::Client::from_java_1_21_11(miner.api.bot.clone())
        .survival()
        .unwrap();
    let independent = crate::Client::from_java_1_21_11(observer.api.bot.clone())
        .survival()
        .unwrap();
    let observed_recovery = source
        .prepare_mining_retirement(&intent, &independent)
        .await
        .unwrap();
    let recovery = source
        .prepare_mining_profile_recovery(&intent)
        .await
        .unwrap();
    recovery.close_source().await.unwrap();
    observer.remove_profile(42).await;
    let config = ConnectionConfig::offline(endpoint, "Miner42", MinecraftVersion::Java1_21_11);
    let mut attempt =
        Box::pin(recovery.reconnect(config.clone(), MiningRecoveryTarget::OriginalOrAir));
    let (mut login, _) = timeout(Duration::from_secs(1), async {
        tokio::select! {
            accepted = listener.accept() => accepted.unwrap(),
            _ = &mut attempt => panic!("recovery returned before fixture login"),
        }
    })
    .await
    .unwrap();
    let handshake = timeout(Duration::from_secs(1), async {
        tokio::select! {
            packet = read_packet(&mut login, None) => packet.unwrap(),
            _ = &mut attempt => panic!("recovery returned before handshake"),
        }
    })
    .await
    .unwrap();
    assert_eq!(handshake.0, 0);
    drop(attempt);
    let history = recovery.source_history().await;
    assert!(history.connection_closed);
    assert_eq!(
        history.mining.unwrap().recovery_attempt.unwrap().method,
        MiningRecoveryMethod::SameProfileLogin
    );
    assert!(
        recovery
            .clone()
            .reconnect(config.clone(), MiningRecoveryTarget::OriginalOrAir)
            .await
            .is_err()
    );
    assert!(
        source
            .prepare_mining_profile_recovery(&intent)
            .await
            .is_err()
    );
    assert!(
        observed_recovery
            .reconnect(config, native("stone"))
            .await
            .is_err()
    );
    assert!(
        timeout(Duration::from_millis(10), listener.accept())
            .await
            .is_err()
    );
    observer.stop().await;
    miner.stop().await;
}

#[tokio::test]
async fn profile_recovery_cannot_retry_an_independent_login_attempt() {
    let mut miner = Fixture::new().await;
    let intent = miner.start().await;
    let source = crate::Client::from_java_1_21_11(miner.api.bot.clone())
        .survival()
        .unwrap();
    let recovery = source
        .prepare_mining_profile_recovery(&intent)
        .await
        .unwrap();
    recovery.close_source().await.unwrap();
    let identity = miner.session.state.lock().await.identity.clone().unwrap();
    miner
        .api
        .claim_mining_recovery(
            &intent,
            &identity,
            MiningRecoveryMethod::IndependentRemoval,
            MiningRecoveryTarget::OriginalOrAir,
        )
        .await
        .unwrap();
    assert!(
        recovery
            .reconnect(
                ConnectionConfig::offline(
                    identity.server,
                    "Miner42",
                    MinecraftVersion::Java1_21_11
                ),
                MiningRecoveryTarget::OriginalOrAir
            )
            .await
            .is_err()
    );
    miner.stop().await;
}
