use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, Write};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::{SinkExt, StreamExt};

#[derive(Debug, Clone)]
struct Group {
    id: String,
    name: String,
    parent: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 29er CLI - Nostr Groups Explorer");
    println!("====================================\n");

    // Get nsec from user
    print!("Enter your Nostr private key (nsec): ");
    io::stdout().flush()?;

    let mut nsec_input = String::new();
    io::stdin().read_line(&mut nsec_input)?;
    let _nsec_input = nsec_input.trim();

    // For now, just show the pubkey from nsec (simplified)
    println!("✓ Received nsec");
    println!("Connecting to nip29.f7z.io...\n");

    // Connect to relay
    let relay_url = "wss://nip29.f7z.io";
    match connect_async(relay_url).await {
        Ok((ws_stream, _)) => {
            println!("✓ Connected to relay");
            println!("📡 Subscribing to group metadata (kind 39000-39003)...\n");

            let (mut write, mut read) = ws_stream.split();

            // Send subscription for group metadata
            let sub = json!([
                "REQ",
                "groups",
                { "kinds": [39000, 39001, 39002, 39003] }
            ]);

            write.send(Message::Text(sub.to_string())).await?;

            println!("Channels:\n");

            let mut groups: BTreeMap<String, Group> = BTreeMap::new();
            let mut event_count = 0;

            // Listen for events
            while let Some(msg) = read.next().await {
                match msg? {
                    Message::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            if let Some(msg_type) = value.get(0).and_then(|v| v.as_str()) {
                                if msg_type == "EVENT" {
                                    event_count += 1;

                                    // Extract event data
                                    if let Some(event) = value.get(2).and_then(|v| v.as_object()) {
                                        let kind = event.get("kind").and_then(|v| v.as_u64()).unwrap_or(0);

                                        // Check if it's a group metadata event
                                        if (39000..=39003).contains(&kind) {
                                            let content = event.get("content")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("");

                                            // Parse tags for group id and parent
                                            let empty_tags = vec![];
                                            let tags = event.get("tags")
                                                .and_then(|v| v.as_array())
                                                .unwrap_or(&empty_tags);

                                            let group_id = tags.iter()
                                                .find_map(|tag| {
                                                    if let Some(arr) = tag.as_array() {
                                                        if arr.len() > 0 {
                                                            if let Some("d") = arr.get(0).and_then(|v| v.as_str()) {
                                                                return arr.get(1).and_then(|v| v.as_str()).map(|s| s.to_string());
                                                            }
                                                        }
                                                    }
                                                    None
                                                });

                                            if let Some(id) = group_id {
                                                let parent = tags.iter()
                                                    .find_map(|tag| {
                                                        if let Some(arr) = tag.as_array() {
                                                            if arr.len() > 0 {
                                                                if let Some("a") = arr.get(0).and_then(|v| v.as_str()) {
                                                                    return arr.get(2).and_then(|v| v.as_str()).map(|s| s.to_string());
                                                                }
                                                            }
                                                        }
                                                        None
                                                    });

                                                let group = Group {
                                                    id: id.clone(),
                                                    name: content.to_string(),
                                                    parent: parent.clone(),
                                                };

                                                groups.insert(id.clone(), group);

                                                // Display update
                                                let prefix = if parent.is_some() { "  └─ " } else { "📦 " };
                                                let display_name = if content.is_empty() { &id } else { content };
                                                println!("{}{}", prefix, display_name);
                                            }
                                        }
                                    }
                                } else if msg_type == "EOSE" {
                                    println!("\n✓ Loaded {} group events", event_count);
                                    println!("\nWaiting for new channels...\n");
                                }
                            }
                        }
                    }
                    Message::Close(_) => {
                        println!("\nRelay connection closed");
                        break;
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to relay: {}", e);
        }
    }

    Ok(())
}
