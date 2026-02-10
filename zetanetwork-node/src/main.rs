// src/main.rs — Zeta Network Node
// P2P (gossipsub) + WebSocket bridge pour navigateurs

use libp2p::{
    gossipsub, identify, noise, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, PeerId,
};
use futures::{SinkExt, StreamExt};
use std::{error::Error, time::Duration};
use tokio::{
    net::TcpListener as TokioTcpListener,
    sync::{broadcast, mpsc},
    time::interval,
};
use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};

#[derive(NetworkBehaviour)]
struct ZetaBehaviour {
    gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🚀 Démarrage du nœud Zeta Network...");

    // Canal broadcast pour les clients WebSocket
    let (ws_broadcast_tx, _) = broadcast::channel::<String>(1000);
    // Canal pour envoyer les messages WS vers gossipsub
    let (ws_to_gossip_tx, mut ws_to_gossip_rx) = mpsc::channel::<String>(100);

    // Construction du swarm libp2p
    let mut swarm = libp2p::SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_tcp(
            tcp::Config::default(),
            noise::Config::new,
            yamux::Config::default,
        )?
        .with_behaviour(|key| {
            let gossipsub_config = gossipsub::ConfigBuilder::default()
                .max_transmit_size(2 * 1024 * 1024) // 2 Mo
                .build()
                .expect("config gossipsub valide");

            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )
            .expect("behaviour gossipsub valide");

            Result::<_, std::io::Error>::Ok(ZetaBehaviour {
                gossipsub,
                identify: identify::Behaviour::new(identify::Config::new(
                    "/zetanetwork/1.0.0".into(),
                    key.public(),
                )),
                ping: ping::Behaviour::new(ping::Config::new()),
            })
        })?
        .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(300)))
        .build();

    let peer_id = *swarm.local_peer_id();
    println!("🔑 PeerId local: {peer_id}");

    // Écouter sur TCP pour le P2P inter-nœuds
    swarm.listen_on("/ip4/0.0.0.0/tcp/9090".parse()?)?;

    // S'abonner au topic gossipsub
    let topic = gossipsub::IdentTopic::new("global-feed");
    swarm.behaviour_mut().gossipsub.subscribe(&topic)?;

    // ── Serveur WebSocket pour les navigateurs ──
    let ws_tx = ws_broadcast_tx.clone();
    let ws_gossip_tx = ws_to_gossip_tx.clone();
    tokio::spawn(async move {
        let listener = TokioTcpListener::bind("0.0.0.0:9091")
            .await
            .expect("Impossible de bind le port 9091");
        println!("🌐 WebSocket en écoute sur 0.0.0.0:9091");

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    let tx = ws_tx.clone();
                    let to_gossip = ws_gossip_tx.clone();
                    tokio::spawn(async move {
                        let Ok(ws) = accept_async(stream).await else {
                            return;
                        };
                        println!("📱 +Navigateur connecté: {addr}");
                        let (mut sink, mut stream) = ws.split();
                        let mut rx = tx.subscribe();

                        // Tâche: relayer les broadcasts vers ce navigateur
                        let fwd = tokio::spawn(async move {
                            while let Ok(msg) = rx.recv().await {
                                if sink.send(WsMessage::Text(msg)).await.is_err() {
                                    break;
                                }
                            }
                        });

                        // Lire les messages du navigateur
                        while let Some(Ok(msg)) = stream.next().await {
                            if let WsMessage::Text(text) = msg {
                                // Broadcast à tous les clients WS
                                let _ = tx.send(text.clone());
                                // Publier sur gossipsub (pour les autres nœuds)
                                let _ = to_gossip.send(text).await;
                            }
                        }

                        fwd.abort();
                        println!("📱 -Navigateur déconnecté: {addr}");
                    });
                }
                Err(e) => eprintln!("Erreur accept WS: {e}"),
            }
        }
    });

    // ── Boucle principale ──
    let mut tick = interval(Duration::from_secs(30));
    let mut peer_count: usize = 0;

    loop {
        tokio::select! {
            _ = tick.tick() => {
                println!("📊 Pairs P2P: {peer_count}");
            }
            Some(text) = ws_to_gossip_rx.recv() => {
                // Publier le message du navigateur sur gossipsub
                if let Err(e) = swarm.behaviour_mut().gossipsub.publish(
                    topic.clone(), text.as_bytes()
                ) {
                    // Normal avec 0 pair: InsufficientPeers
                    eprintln!("gossipsub publish (attendu si aucun pair): {e}");
                }
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(ZetaBehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message, .. }
                )) => {
                    if let Ok(text) = std::str::from_utf8(&message.data) {
                        println!("📨 gossip: {text}");
                        // Relayer aux navigateurs connectés
                        let _ = ws_broadcast_tx.send(text.to_string());
                    }
                }
                SwarmEvent::ConnectionEstablished { .. } => {
                    peer_count += 1;
                    println!("✅ Nouveau pair! Total: {peer_count}");
                }
                SwarmEvent::ConnectionClosed { .. } => {
                    peer_count = peer_count.saturating_sub(1);
                    println!("❌ Pair déconnecté. Total: {peer_count}");
                }
                SwarmEvent::NewListenAddr { address, .. } => {
                    println!("📡 Écoute sur: {address}/p2p/{peer_id}");
                }
                _ => {}
            }
        }
    }
}