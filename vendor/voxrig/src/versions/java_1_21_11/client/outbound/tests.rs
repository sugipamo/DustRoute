use super::*;
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::io::{AsyncReadExt, AsyncWrite};

async fn fixture() -> (Arc<Session>, OwnedReadHalf, TcpStream) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (peer, _) = listener.accept().await.unwrap();
    let (reader, writer) = client.into_split();
    let session = Arc::new(Session {
        id: 42,
        started: Instant::now(),
        writer: Mutex::new(Writer {
            stream: writer,
            compression: None,
        }),
        state: Mutex::new(State::default()),
        changed: Notify::new(),
        cancel: Notify::new(),
        stopped: AtomicBool::new(false),
        interrupted_packet: AtomicI32::new(-1),
        limits: crate::ConnectionOptions::default(),
        interaction_sequence: AtomicI32::new(0),
    });
    (session, reader, peer)
}

async fn pending<F: Future>(mut future: Pin<&mut F>) {
    std::future::poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

fn operations(session: &Arc<Session>) -> operations::Operations {
    operations::Operations {
        bot: Bot {
            session: session.clone(),
            _lease: Arc::new(Lease(Arc::downgrade(session))),
        },
    }
}

#[tokio::test]
async fn completed_frames_decode_with_single_byte_fragmentation_and_compression() {
    let (session, _, _) = fixture().await;
    for compression in [None, Some(0), Some(256)] {
        let (mut writer, mut reader) = tokio::io::duplex(1);
        let sender = async {
            for id in [1, 2] {
                session
                    .write_frame(&mut writer, compression, id, &[42; 512])
                    .await
                    .unwrap();
            }
        };
        let receiver = async {
            for id in [1, 2] {
                let result = read_packet(&mut reader, compression).await.unwrap();
                assert_eq!(result, (id, vec![42; 512]));
            }
        };
        timeout(Duration::from_secs(2), async {
            tokio::join!(sender, receiver);
        })
        .await
        .unwrap();
        assert!(!session.stopped.load(Ordering::Acquire));
        assert_eq!(session.interrupted_packet.load(Ordering::Acquire), -1);
    }
}

#[tokio::test]
async fn cancelling_lock_wait_does_not_poison_or_send() {
    let (session, _, mut peer) = fixture().await;
    let locked = session.writer.lock().await;
    let mut first = Box::pin(session.send(1, &[2]));
    pending(first.as_mut()).await;
    drop(first);
    assert!(!session.stopped.load(Ordering::Acquire));
    assert_eq!(session.interrupted_packet.load(Ordering::Acquire), -1);
    drop(locked);
    session.send(3, &[4]).await.unwrap();
    assert_eq!(read_packet(&mut peer, None).await.unwrap(), (3, vec![4]));
}

#[tokio::test]
async fn cancelled_partial_frame_prevents_queued_requests_and_automatic_response_and_closes_tcp() {
    let (session, reader, mut peer) = fixture().await;
    let running = session.clone();
    let receiver = tokio::spawn(async move {
        running.run_receiver(reader).await;
    });
    write_packet(
        &mut peer,
        None,
        ids::configuration_clientbound::PING,
        &[0; 4],
    )
    .await
    .unwrap();
    assert_eq!(
        read_packet(&mut peer, None).await.unwrap().0,
        ids::configuration_serverbound::PONG
    );
    // Keep the real writer lock, but deterministically interrupt its common
    // framing helper on a one-byte duplex stream instead of saturating TCP.
    let locked = session.writer.lock().await;
    let (mut limited, mut received) = tokio::io::duplex(1);
    let mut attempt = Box::pin(session.write_frame(&mut limited, None, 5, &[0; 100]));
    pending(attempt.as_mut()).await;
    let mut queued = Box::pin(session.send(6, &[7]));
    pending(queued.as_mut()).await;
    write_packet(
        &mut peer,
        None,
        ids::configuration_clientbound::KEEP_ALIVE,
        &[0; 8],
    )
    .await
    .unwrap();
    // Receiving the request proves the automatic response will use the same
    // writer queue. It must not append bytes after the interrupted helper.
    timeout(Duration::from_secs(1), async {
        loop {
            if session.state.lock().await.sequence == 2 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(attempt);
    assert_eq!(session.interrupted_packet.load(Ordering::Acquire), 5);
    drop(locked);
    assert_eq!(
        queued.await.unwrap_err().kind(),
        ErrorKind::UncertainDispatch
    );
    assert_eq!(
        session.send(8, &[]).await.unwrap_err().kind(),
        ErrorKind::UncertainDispatch
    );
    timeout(Duration::from_secs(1), receiver)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(1), peer.read_u8())
            .await
            .unwrap()
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::UnexpectedEof
    );
    drop(limited);
    let mut bytes = vec![];
    received.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, vec![101]); // original frame prefix only; no next packet
}

struct FailingWriter {
    bytes: Vec<u8>,
    limit: usize,
    fail_flush: bool,
}
impl AsyncWrite for FailingWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
        input: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.bytes.len() == self.limit {
            return Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()));
        }
        let count = input.len().min(self.limit - self.bytes.len());
        self.bytes.extend_from_slice(&input[..count]);
        Poll::Ready(Ok(count))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(if self.fail_flush {
            Err(std::io::ErrorKind::BrokenPipe.into())
        } else {
            Ok(())
        })
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn write_or_flush_error_preserves_first_uncertainty_and_never_appends() {
    for (limit, fail_flush) in [(0, false), (2, false), (100, true)] {
        let (session, _, _) = fixture().await;
        let mut writer = FailingWriter {
            bytes: vec![],
            limit,
            fail_flush,
        };
        assert_eq!(
            session
                .write_frame(&mut writer, None, 9, &[10; 8])
                .await
                .unwrap_err()
                .kind(),
            ErrorKind::UncertainDispatch
        );
        let before = writer.bytes.clone();
        assert_eq!(
            session
                .write_frame(&mut writer, None, 11, &[])
                .await
                .unwrap_err()
                .kind(),
            ErrorKind::UncertainDispatch
        );
        assert_eq!(writer.bytes, before);
        assert_eq!(session.interrupted_packet.load(Ordering::Acquire), 9);
        assert!(session.stopped.load(Ordering::Acquire));
    }
}

#[tokio::test]
async fn explicit_disconnect_interrupts_a_blocked_frame_and_retains_inventory_history() {
    let (session, reader, mut peer) = fixture().await;
    let api = operations(&session);
    {
        let mut state = session.state.lock().await;
        state.ready = true;
        state.sequence = 3;
        state.loading = loading::InteractionLoading::completed_fixture();
        state.operations.reset_world(0).unwrap();
        let mut full = vec![0, 1, 46];
        for slot in 0..46 {
            if slot == 9 {
                full.push(1);
                put_varint(
                    &mut full,
                    operations::default_item("stone", 1).unwrap().item_id,
                );
                full.extend([0, 0]);
            } else {
                full.push(0);
            }
        }
        full.push(0);
        operations::receive(&mut state, ids::play_clientbound::WINDOW_ITEMS, &full).unwrap();
    }
    let running = session.clone();
    let receiver = tokio::spawn(async move {
        running.run_receiver(reader).await;
    });
    let locked = session.writer.lock().await;
    let mut swap_attempt = Box::pin(api.swap_player_hotbar(9, 0));
    pending(swap_attempt.as_mut()).await;
    drop(swap_attempt);
    let swap = api
        .operation_history()
        .await
        .pending_inventory_swap
        .unwrap();
    let (mut limited, _limited_reader) = tokio::io::duplex(1);
    let mut attempt = Box::pin(session.write_frame(
        &mut limited,
        None,
        ids::play_serverbound::WINDOW_CLICK,
        &[0; 100],
    ));
    pending(attempt.as_mut()).await;
    session.stop();
    assert_eq!(
        attempt.await.unwrap_err().kind(),
        ErrorKind::UncertainDispatch
    );
    drop(locked);
    timeout(Duration::from_secs(1), receiver)
        .await
        .unwrap()
        .unwrap();
    assert!(peer.read_u8().await.is_err());
    assert_eq!(
        api.player_state().await.unwrap_err().kind(),
        ErrorKind::UncertainDispatch
    );
    let history = api.operation_history().await;
    assert!(history.connection_closed);
    assert_eq!(
        history.interrupted_packet_id,
        Some(ids::play_serverbound::WINDOW_CLICK)
    );
    assert_eq!(history.pending_inventory_swap, Some(swap));
    assert!(api.select_hotbar(0).await.is_err());
    assert!(
        api.operation_history()
            .await
            .pending_inventory_swap
            .is_some()
    );
}

#[tokio::test]
async fn creative_intent_is_retained_when_submission_is_cancelled_before_writer_acquisition() {
    let (session, _, _) = fixture().await;
    let api = operations(&session);
    {
        let mut state = session.state.lock().await;
        state.ready = true;
        state.operations.reset_world(1).unwrap();
        state.loading = loading::InteractionLoading::completed_fixture();
    }
    let locked = session.writer.lock().await;
    let mut attempt = Box::pin(api.set_creative_hotbar(0, Some(("stone", 1))));
    pending(attempt.as_mut()).await;
    drop(attempt);
    drop(locked);
    session.stop();
    let history = api.operation_history().await;
    assert!(history.connection_closed);
    assert_eq!(history.interrupted_packet_id, None);
    assert_eq!(history.pending_creative_slots, vec![0]);
}

#[tokio::test]
async fn position_attempt_retains_receipt_and_cancelled_dispatch_blocks_next_mutation() {
    use operations::{PositionBasis, ReceivedPose};
    let (session, _, mut peer) = fixture().await;
    let api = operations(&session);
    {
        let mut s = session.state.lock().await;
        s.phase = Phase::Play;
        s.ready = true;
        s.loading = loading::InteractionLoading::completed_fixture();
        s.position = Some([0.5, 1.0, 0.5]);
        s.operations.reset_world(1).unwrap();
        let mut abilities = vec![4];
        abilities.extend(0.05f32.to_be_bytes());
        abilities.extend(0.1f32.to_be_bytes());
        operations::receive(&mut s, ids::play_clientbound::ABILITIES, &abilities).unwrap();
        let generation = s.loading.generation;
        s.motion.receive(ReceivedPose {
            generation,
            receive_sequence: 5,
            position: [0.5, 1.0, 0.5],
            rotation: [0.0; 2],
            velocity: Some([0.0; 3]),
        });
    }
    api.set_flying(true).await.unwrap();
    assert_eq!(
        read_packet(&mut peer, None).await.unwrap().0,
        ids::play_serverbound::ABILITIES
    );
    api.move_flying([1.5, 1.0, 0.5], [0.0; 2]).await.unwrap();
    assert_eq!(
        read_packet(&mut peer, None).await.unwrap().0,
        ids::play_serverbound::POSITION_LOOK
    );
    let sent = api.player_state().await.unwrap();
    assert!(!sent.position_from_server);
    assert_eq!(sent.motion.position_basis, PositionBasis::Submitted);
    assert_eq!(sent.motion.received_pose.unwrap().position, [0.5, 1.0, 0.5]);
    assert!(sent.motion.last_submission.unwrap().dispatched);
    let locked = session.writer.lock().await;
    let mut request = Box::pin(api.move_flying([2.5, 1.0, 0.5], [0.0; 2]));
    pending(request.as_mut()).await;
    drop(request);
    drop(locked);
    let history = api.operation_history().await;
    assert_eq!(
        history.motion.position_basis,
        PositionBasis::PendingSubmission
    );
    assert!(!history.motion.last_submission.as_ref().unwrap().dispatched);
    assert_eq!(history.motion.last_submission.unwrap().attempt_id, 2);
    assert!(api.select_hotbar(1).await.is_err());
    assert!(api.move_flying([2.5, 1.0, 0.5], [0.0; 2]).await.is_err());
    assert!(
        timeout(Duration::from_millis(10), read_packet(&mut peer, None))
            .await
            .is_err()
    );
    {
        let mut s = session.state.lock().await;
        let generation = s.loading.generation;
        s.position = Some([0.5, 1.0, 0.5]);
        s.motion.receive(ReceivedPose {
            generation,
            receive_sequence: 9,
            position: [0.5, 1.0, 0.5],
            rotation: [0.0; 2],
            velocity: Some([0.0; 3]),
        });
    }
    let restored = api.player_state().await.unwrap();
    assert!(restored.position_from_server);
    assert_eq!(
        restored.motion.last_submission.unwrap().superseded_at,
        Some(9)
    );
    {
        let mut s = session.state.lock().await;
        s.loading.reset(10);
        s.motion.invalidate(10, "world generation changed");
    }
    let reset = api.player_state().await.unwrap();
    assert!(!reset.position_from_server);
    assert_eq!(reset.motion.received_pose.unwrap().receive_sequence, 9);
    assert_eq!(reset.motion.position_basis, PositionBasis::Unavailable);
}
