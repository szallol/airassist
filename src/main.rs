use std::process;

use anyhow::Result;

use futures::StreamExt;
use paho_mqtt as mqtt;
use serde::{Deserialize, Serialize};
use serde_xml_rs::from_str;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::{task, time};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Response {
    #[serde(rename = "stufe1")]
    level1: u8,
    #[serde(rename = "stufe2")]
    level2: u8,
    #[serde(rename = "stufe3")]
    level3: u8,
    #[serde(rename = "stufe4")]
    level4: u8,
    #[serde(rename = "abl0")]
    exhaust: f32,
    #[serde(rename = "zul0")]
    fresh: f32,
    #[serde(rename = "aul0")]
    outdoor: f32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let host = "mqtt://localhost:1883".to_string();
    // Create the mqtt client
    let mut cli = mqtt::AsyncClient::new(host).unwrap_or_else(|_| {
        // println!("Error creating the client: {}", err);
        process::exit(1);
    });
    let mut strm = cli.get_stream(25);

    let cli = Arc::new(Mutex::new(cli));

    cli.lock().await.connect(None).await?;
    cli.lock()
        .await
        .subscribe("air/cmd/level", mqtt::QOS_1)
        .await?;

    let cli_pub = cli.clone();
    let forever1 = task::spawn(async move {
        let _ = publish_config(cli_pub.clone()).await;
        let mut interval = time::interval(std::time::Duration::from_secs(60));

        loop {
            interval.tick().await;
            let _ = publish_stats(cli_pub.clone()).await;
        }
    });

    let forever2 = task::spawn(async move {
        while let Some(msg_opt) = strm.next().await {
            if let Some(msg) = msg_opt {
                // println!("{}", msg);
                let _ = set_level(&msg.payload_str()).await;
            } else {
                // A "None" means we were disconnected. Try to reconnect...
                // println!("Lost connection. Attempting reconnect...");
                while let Err(_) = cli.lock().await.reconnect().await {
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                }
                // println!("Reconnected.");
            }
        }
    });

    tokio::try_join!(forever1, forever2)?;

    Ok(())
}

async fn set_level(level: &str) -> Result<()> {
    let url = format!("http://192.168.100.64/stufe.cgi?stufe={}", level);
    // println!("set_level url: {}", &url);
    let _ = reqwest::get(url).await?.text().await?;

    // println!("response body= {:?}", body);
    Ok(())
}

async fn publish_stats(cli: Arc<Mutex<mqtt::AsyncClient>>) -> Result<()> {
    let body = reqwest::get("http://192.168.100.64/status.xml")
        .await?
        .text()
        .await?;

    let responsexml: Response = from_str(&body).unwrap();
    // println!("responsexml = {:?}", responsexml);

    let msg = mqtt::Message::new(
        "air/status/exhaust",
        format!("{}", responsexml.exhaust),
        mqtt::QOS_1,
    );
    cli.lock().await.publish(msg).await?;

    let msg = mqtt::Message::new(
        "air/status/fresh",
        format!("{}", responsexml.fresh),
        mqtt::QOS_1,
    );
    cli.lock().await.publish(msg).await?;

    let msg = mqtt::Message::new(
        "air/status/outdoor",
        format!("{}", responsexml.outdoor),
        mqtt::QOS_1,
    );
    cli.lock().await.publish(msg).await?;
    Ok(())
}

async fn publish_config(cli: Arc<Mutex<mqtt::AsyncClient>>) -> Result<()> {
    let msg = mqtt::Message::new(
        "homeassistant/sensor/air_1_exhaust/config",
        r#"
        {
            "state_topic" : "air/status/exhaust", 
            "name" : "Exhaust temperature", 
            "state_class": "measurement",
            "unique_id" : "air_1_exhaust"
        }
        "#,
        mqtt::QOS_1,
    );
    cli.lock().await.publish(msg).await?;

    let msg = mqtt::Message::new(
        "homeassistant/sensor/air_1_fresh/config",
        r#"
        {
            "state_topic" : "air/status/fresh",
            "name" : "Fresh temperature",
            "state_class": "measurement", 
            "unique_id" : "air_1_fresh"
        }
        "#,
        mqtt::QOS_1,
    );
    cli.lock().await.publish(msg).await?;

    let msg = mqtt::Message::new(
        "homeassistant/sensor/air_1_outdoor/config",
        r#"
        {
            "state_topic" : "air/status/outdoor",
            "name" : "Outdoor temperature", 
            "state_class": "measurement",
            "unique_id" : "air_1_autdoor"
        }
        "#,
        mqtt::QOS_1,
    );
    cli.lock().await.publish(msg).await?;

    let msg = mqtt::Message::new(
        "homeassistant/select/air_1_level/config",
        r#"
        {
            "command_topic" : "air/cmd/level",
            "name" : "Ventilation level",
            "options" : ["1", "2", "3"],
            "unique_id" : "air_1_level"
        }
        "#,
        mqtt::QOS_1,
    );
    cli.lock().await.publish(msg).await?;

    Ok(())
}
