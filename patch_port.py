import sys
with open('crates/hitz-daemon/src/port_forward.rs', 'r') as f:
    content = f.read()

import re

search = '''
            let handle = tokio::spawn(async move {
                loop {
                    match listener.accept().await {
                        Ok((mut inbound, _peer)) => {
                            connections_total_l.add(1, std::slice::from_ref(&kv));
                            relays_active_l.add(1, std::slice::from_ref(&kv));

                            let relays_active_r = relays_active_l.clone();
                            let kv_r = kv.clone();
                            let relay = tokio::spawn(async move {
                                match tokio::net::TcpStream::connect(guest_addr).await {
                                    Ok(mut outbound) => {
                                        let _ = tokio::io::copy_bidirectional(
                                            &mut inbound,
                                            &mut outbound,
                                        )
                                        .await;
                                    }
                                    Err(e) => {
                                        tracing::debug!(
                                            "port forward connect to guest failed: {e}"
                                        );
                                    }
                                }
                                // Relay complete — decrement active counter.
                                relays_active_r.add(-1, &[kv_r]);
                            });
                            relay_handles_clone.lock().await.push(relay);
                        }
                        Err(e) => {
                            tracing::warn!("port forward accept error, retrying: {e}");
                        }
                    }
                }
            });
'''

replace = '''
            let handle = tokio::spawn(async move {
                loop {
                    let Ok((mut inbound, _peer)) = listener.accept().await else {
                        tracing::warn!("port forward accept error, retrying");
                        continue;
                    };

                    connections_total_l.add(1, std::slice::from_ref(&kv));
                    relays_active_l.add(1, std::slice::from_ref(&kv));

                    let relays_active_r = relays_active_l.clone();
                    let kv_r = kv.clone();
                    let relay = tokio::spawn(async move {
                        if let Ok(mut outbound) = tokio::net::TcpStream::connect(guest_addr).await {
                            let _ = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await;
                        } else {
                            tracing::debug!("port forward connect to guest failed");
                        }
                        // Relay complete — decrement active counter.
                        relays_active_r.add(-1, &[kv_r]);
                    });
                    relay_handles_clone.lock().await.push(relay);
                }
            });
'''

new_content = content.replace(search.strip(), replace.strip())

with open('crates/hitz-daemon/src/port_forward.rs', 'w') as f:
    f.write(new_content)
