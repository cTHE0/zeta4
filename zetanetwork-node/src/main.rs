// src/main.rs
use libp2p::{
    gossipsub, identify, noise, ping,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, Transport,
};
use std::{collections::HashSet, error::Error, time::Duration};
use tokio::time::interval;

// Comportement réseau combiné
#[derive(NetworkBehaviour)]
struct Behaviour {
    gossipsub: gossipsub::Behaviour,
    identify: identify::Behaviour,
    ping: ping::Behaviour,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 1. Créer une identité
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let peer_id = PeerId::from(keypair.public());

    // 2. Construire le transport
    let transport = libp2p::development_transport(keypair.clone()).await?;

    // 3. Configurer Gossipsub
    let mut gossipsub_config = gossipsub::ConfigBuilder::default();
    gossipsub_config.max_transmit_size(2 * 1024 * 1024); // 2 Mo
    let gossipsub = gossipsub::Behaviour::new(
        gossipsub::MessageAuthenticity::Signed(keypair.clone()),
        gossipsub_config.build().unwrap(),
    )?;

    // 4. Créer le swarm
    let behaviour = Behaviour {
        gossipsub,
        identify: identify::Behaviour::new(identify::Config::new(
            "/zetanetwork/1.0.0".into(),
            keypair.public(),
        )),
        ping: ping::Behaviour::new(ping::Config::new()),
    };

    let mut swarm = Swarm::new(transport, behaviour, peer_id);

    // 5. Écouter sur TCP + WebSocket (pour les navigateurs)
    swarm.listen_on("/ip4/0.0.0.0/tcp/9090".parse()?)?;
    swarm.listen_on("/ip4/0.0.0.0/tcp/9091/ws".parse()?)?; // WebSocket pour frontend

    // 6. Se connecter aux autres VPS (bootstrap)
    let bootstrap_nodes = vec![
        "/ip4/65.75.201.11/tcp/9090/p2p/QmVPS1",
        "/ip4/65.75.200.180/tcp/9090/p2p/QmVPS2",
    ];
    for addr in bootstrap_nodes {
        if let Ok(ma) = addr.parse::<Multiaddr>() {
            swarm.dial(ma)?;
        }
    }

    // 7. S'abonner au topic global
    swarm.behaviour_mut().gossipsub.subscribe("global-feed".as_bytes())?;

    // 8. Boucle principale
    let mut ping_interval = interval(Duration::from_secs(30));
    let mut connected_peers = HashSet::new();

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                println!("Nœuds connectés: {}", connected_peers.len());
            }
            event = swarm.select_next_some() => match event {
                SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(
                    gossipsub::Event::Message { message, .. }
                )) => {
                    if let Ok(text) = std::str::from_utf8(&message.data) {
                        println!("📨 {}: {}", message.source.unwrap_or_default(), text);
                    }
                }
                SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                    connected_peers.insert(peer_id);
                }
                SwarmEvent::ConnectionClosed { peer_id, .. } => {
                    connected_peers.remove(&peer_id);
                }
                _ => {}
            }
        }
    }
}