//! One Tokio actor owns WebSocket RPC. The UI awaits bounded events, never polls.
use crate::{
    media::{self, Attachment},
    model::{MAX_FRAME, PAGE, clean, number, text},
};
use anyhow::{Context, Result, bail, ensure};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use tokio::{
    net::TcpStream,
    sync::mpsc,
    time::{interval, timeout},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{Message as Frame, protocol::WebSocketConfig},
};
#[derive(Debug)]
pub enum Command {
    History {
        generation: u64,
        room: String,
        before: Option<u64>,
    },
    Send {
        generation: u64,
        room: String,
        text: String,
        reply: Option<u64>,
        files: Vec<Attachment>,
    },
    ReadFiles {
        generation: u64,
        paths: Vec<PathBuf>,
    },
    Download {
        generation: u64,
        url: String,
        path: PathBuf,
    },
    Join {
        room: String,
    },
    Reconnect,
    CancelTransfer,
}
#[derive(Debug)]
pub enum Event {
    State(String),
    Ready {
        you: String,
        rooms: Value,
        caps: Vec<String>,
    },
    Notice(Value),
    History {
        generation: u64,
        room: String,
        page: Value,
    },
    Sent {
        generation: u64,
        text: String,
        reply: Option<u64>,
    },
    Files {
        generation: u64,
        files: Vec<Attachment>,
    },
    Failure {
        generation: Option<u64>,
        message: String,
    },
    Transfer {
        message: String,
        busy: bool,
    },
}
pub struct Bridge {
    pub commands: mpsc::Sender<Command>,
    pub events: async_channel::Receiver<Event>,
}
impl Bridge {
    pub fn start(url: String) -> Result<Self> {
        media::endpoint(&url)?;
        let (tx, rx) = mpsc::channel(16);
        let (etx, erx) = async_channel::bounded(128);
        std::thread::Builder::new()
            .name("apron-tokio".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("Tokio runtime");
                runtime.block_on(run(url, rx, etx));
            })?;
        Ok(Self {
            commands: tx,
            events: erx,
        })
    }
}
#[derive(Debug)]
struct RemoteError(String);
impl std::fmt::Display for RemoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for RemoteError {}

pub struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    next: u64,
    pub server: Value,
    awaiting_pong: bool,
    events: Option<async_channel::Sender<Event>>,
}
impl Client {
    pub async fn connect(url: &str, events: Option<async_channel::Sender<Event>>) -> Result<Self> {
        media::endpoint(url)?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_FRAME))
            .max_frame_size(Some(MAX_FRAME));
        let (ws, _) = timeout(
            Duration::from_secs(15),
            tokio_tungstenite::connect_async_with_config(url, Some(config), false),
        )
        .await??;
        let mut c = Self {
            ws,
            next: 0,
            server: Value::Null,
            awaiting_pong: false,
            events,
        };
        let hello = timeout(Duration::from_secs(15), c.receive()).await??;
        ensure!(
            hello["method"] == "server" && hello["params"]["apron"] == 8,
            "Server must speak Apron 8"
        );
        c.server = hello["params"].clone();
        Ok(c)
    }
    async fn receive(&mut self) -> Result<Value> {
        loop {
            match self.ws.next().await.context("Connection closed")?? {
                Frame::Text(s) => {
                    let frame: Value = serde_json::from_str(&s)?;
                    if frame["method"] == "pong" {
                        self.awaiting_pong = false;
                    }
                    return Ok(frame);
                }
                Frame::Close(_) => bail!("Connection closed"),
                Frame::Ping(v) => self.ws.send(Frame::Pong(v)).await?,
                _ => {}
            }
        }
    }
    async fn notice(&self, frame: Value) -> Result<()> {
        if let Some(e) = &self.events {
            e.send(Event::Notice(frame)).await?;
        }
        Ok(())
    }
    pub async fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next += 1;
        let id = self.next.to_string();
        self.ws
            .send(Frame::Text(
                json!({"id":id,"method":method,"params":params})
                    .to_string()
                    .into(),
            ))
            .await?;
        timeout(Duration::from_secs(20), async {
            loop {
                let f = self.receive().await?;
                if f["id"] == id {
                    if f.get("error").is_some() {
                        return Err(RemoteError(clean(text(&f["error"]["message"]), 400)).into());
                    }
                    return Ok(f["result"].clone());
                }
                self.notice(f).await?;
            }
        })
        .await
        .context("Request timed out; delivery may be uncertain")?
    }
    pub async fn guest(&mut self) -> Result<Value> {
        self.rpc(
            "auth",
            json!({"scheme":"guest","name":"Apron GPUI","agent":"apron-gpui/0.1"}),
        )
        .await
    }
    pub async fn history(&mut self, room: &str, before: Option<u64>) -> Result<Value> {
        let mut params = json!({"room_id":room,"limit":PAGE});
        if let Some(n) = before {
            params["before"] = json!(n.to_string());
        }
        let mut page = self.rpc("history", params).await?;
        if page["messages"].is_null() {
            page["messages"] = json!([]);
        }
        ensure!(
            page["messages"].as_array().is_some_and(|v| v.len() <= PAGE),
            "Invalid bounded history page"
        );
        if before.is_some() {
            let horizon = number(&page["latest_log_id"]).context("Missing history horizon")?;
            let mut pos = number(&page["last_log_id"])
                .unwrap_or(horizon)
                .saturating_add(1);
            let mut projection: BTreeMap<String, Value> = page["messages"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|m| Some((m["message_id"].as_str()?.to_owned(), m.clone())))
                .collect();
            let mut pages = 0;
            while pos <= horizon {
                ensure!(
                    pages < 128,
                    "History reconciliation budget exhausted; reload latest"
                );
                pages += 1;
                let update=self.rpc("history",json!({"room_id":room,"after":pos.to_string(),"before":horizon.to_string(),"limit":PAGE})).await?;
                ensure!(
                    number(&update["history_log_id"]).unwrap_or(0)
                        <= number(&page["first_log_id"]).unwrap_or(pos),
                    "History retention changed; reload latest"
                );
                if let Some(messages) = update["messages"].as_array() {
                    for m in messages {
                        if let Some(old) = projection.get_mut(text(&m["message_id"]))
                            && number(&m["log_id"]) >= number(&old["log_id"])
                        {
                            *old = m.clone();
                        }
                    }
                }
                if update["more"] != true {
                    break;
                }
                let last = number(&update["last_log_id"]).context("Missing history bound")?;
                ensure!(last >= pos && last <= horizon, "History did not advance");
                pos = last.saturating_add(1);
            }
            page["messages"] = Value::Array(projection.into_values().collect());
        }
        Ok(page)
    }
}
async fn emit(tx: &async_channel::Sender<Event>, e: Event) {
    let _ = tx.send(e).await;
}
async fn run(
    url: String,
    mut commands: mpsc::Receiver<Command>,
    events: async_channel::Sender<Event>,
) {
    let mut delay = 1;
    loop {
        if events.is_closed() || commands.is_closed() {
            return;
        }
        emit(&events, Event::State("Connecting…".into())).await;
        let result = connected(&url, &mut commands, &events).await;
        if commands.is_closed() || events.is_closed() {
            return;
        }
        emit(
            &events,
            Event::State(format!("Disconnected — reconnecting in {delay}s")),
        )
        .await;
        if let Err(e) = result {
            emit(
                &events,
                Event::Failure {
                    generation: None,
                    message: clean(&format!("{e:#}"), 400),
                },
            )
            .await;
        }
        // Queued writes are never replayed across connections.
        while let Ok(c) = commands.try_recv() {
            if let Command::Send { generation, .. } | Command::History { generation, .. } = c {
                emit(
                    &events,
                    Event::Failure {
                        generation: Some(generation),
                        message: "Connection changed; request was not retried.".into(),
                    },
                )
                .await;
            }
        }
        tokio::select! {_ = tokio::time::sleep(Duration::from_secs(delay))=>{},c=commands.recv()=>{if c.is_none(){return;}
        if let Some(Command::Send{generation,..}|Command::History{generation,..})=c{emit(&events,Event::Failure{generation:Some(generation),message:"Offline; request was not sent.".into()}).await;}}}
        delay = (delay * 2).min(30);
    }
}
async fn connected(
    url: &str,
    commands: &mut mpsc::Receiver<Command>,
    events: &async_channel::Sender<Event>,
) -> Result<()> {
    let mut c = Client::connect(url, Some(events.clone())).await?;
    let auth = c.guest().await?;
    let rooms = c
        .rpc("room_list", json!({"filter":"all","members":true}))
        .await?;
    let caps = c.server["capabilities"]
        .as_array()
        .map(|v| {
            v.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    emit(
        events,
        Event::Ready {
            you: text(&auth["you"]["user_id"]).into(),
            rooms,
            caps,
        },
    )
    .await;
    let mut ping = interval(Duration::from_secs(
        c.server["ping"].as_u64().unwrap_or(20).clamp(5, 300),
    ));
    ping.tick().await;
    let mut transfers = tokio::task::JoinSet::new();
    loop {
        tokio::select! {
         _=transfers.join_next(),if !transfers.is_empty()=>{},
         frame=c.receive()=>{let frame=frame?;c.notice(frame).await?;},
         _=ping.tick()=>{ensure!(!c.awaiting_pong,"Server heartbeat timed out");c.awaiting_pong=true;c.ws.send(Frame::Text(json!({"method":"ping"}).to_string().into())).await?;},
         command=commands.recv()=>{let Some(command)=command else{return Ok(())};
          if !handle_command(command, &mut c, url, events, &mut transfers).await? {
              return Ok(());
          }
         }
        }
    }
}
async fn handle_command(
    command: Command,
    c: &mut Client,
    url: &str,
    events: &async_channel::Sender<Event>,
    transfers: &mut tokio::task::JoinSet<()>,
) -> Result<bool> {
    match command {
        Command::Reconnect => return Ok(false),
        Command::CancelTransfer => {
            transfers.abort_all();
            emit(events,Event::Transfer{message:"Transfer cancelled; message may already be posted. Inspect history before retrying.".into(),busy:false}).await;
        }
        Command::Join { room } => {
            if let Err(e) = c.rpc("room_set", json!({"room_id":room,"join":true})).await {
                emit(
                    events,
                    Event::Failure {
                        generation: None,
                        message: e.to_string(),
                    },
                )
                .await;
            } else {
                let rooms = c.rpc("room_list", json!({"filter":"all"})).await?;
                emit(
                    events,
                    Event::Notice(json!({"method":"room_update","params":rooms})),
                )
                .await;
            }
        }
        Command::History {
            generation,
            room,
            before,
        } => match c.history(&room, before).await {
            Ok(page) => {
                emit(
                    events,
                    Event::History {
                        generation,
                        room,
                        page,
                    },
                )
                .await
            }
            Err(e) => {
                emit(
                    events,
                    Event::Failure {
                        generation: Some(generation),
                        message: e.to_string(),
                    },
                )
                .await;
                if e.downcast_ref::<RemoteError>().is_none() {
                    return Err(e);
                }
            }
        },
        Command::Send {
            generation,
            room,
            text,
            reply,
            files,
        } => {
            if !files.is_empty() && !transfers.is_empty() {
                emit(
                    events,
                    Event::Failure {
                        generation: Some(generation),
                        message: "Finish or cancel the current transfer first.".into(),
                    },
                )
                .await;
                return Ok(true);
            }
            let mut params = json!({"room_id":room,"body":{"text":text,"format":"plain"}});
            if let Some(id) = reply {
                params["reply_to"] = json!({"message_id":id.to_string()});
            }
            if !files.is_empty() {
                params["body"]["embeds"] = json!(
                    files
                        .iter()
                        .map(|f| json!({"kind":"upload","title":f.title}))
                        .collect::<Vec<_>>()
                );
            }
            match c.rpc("message", params).await {
                Ok(result) => {
                    emit(
                        events,
                        Event::Sent {
                            generation,
                            text,
                            reply,
                        },
                    )
                    .await;
                    if !files.is_empty() {
                        let events = events.clone();
                        let url = url.to_owned();
                        transfers.spawn(async move{let uploaded=async {
               let embeds=result["embeds"].as_array().context("No upload URLs returned")?;ensure!(embeds.len()==files.len(),"Server did not accept all files");
               let urls=embeds.iter().map(|v|media::transfer_url(text_value(&v["write_url"]),&url).map(|v|v.to_string())).collect::<Result<Vec<_>>>()?;
               for (ix,(file,target)) in files.into_iter().zip(urls).enumerate(){emit(&events,Event::Transfer{message:format!("Uploading file {}…",ix+1),busy:true}).await;media::put(&target,&url,file.bytes).await.with_context(||format!("{ix} uploaded; message already posted. Remaining files stopped"))?;}Ok::<(),anyhow::Error>(())
              }.await;emit(&events,Event::Transfer{message:match uploaded{Ok(())=>"Attachments uploaded".into(),Err(e)=>format!("{e:#}")},busy:false}).await;});
                    }
                }
                Err(e) => {
                    emit(
                        events,
                        Event::Failure {
                            generation: Some(generation),
                            message: if e.downcast_ref::<RemoteError>().is_some() {
                                format!("Message rejected: {e}")
                            } else {
                                format!("{e}. Delivery may be uncertain; inspect history before retrying.")
                            },
                        },
                    )
                    .await;
                    if e.downcast_ref::<RemoteError>().is_none() {
                        return Err(e.context(
                            "Delivery may be uncertain; inspect history before retrying",
                        ));
                    }
                }
            }
        }
        Command::ReadFiles { generation, paths } => match media::read(paths).await {
            Ok(files) => emit(events, Event::Files { generation, files }).await,
            Err(e) => {
                emit(
                    events,
                    Event::Failure {
                        generation: Some(generation),
                        message: e.to_string(),
                    },
                )
                .await
            }
        },
        Command::Download {
            generation: _,
            url: target,
            path,
        } => {
            if !transfers.is_empty() {
                emit(
                    events,
                    Event::Transfer {
                        message: "Finish or cancel the current transfer first.".into(),
                        busy: true,
                    },
                )
                .await;
                return Ok(true);
            }
            let events = events.clone();
            let url = url.to_owned();
            transfers.spawn(async move {
                emit(
                    &events,
                    Event::Transfer {
                        message: "Downloading attachment…".into(),
                        busy: true,
                    },
                )
                .await;
                let result = async {
                    let data = media::get(&target, &url).await?;
                    media::save(&path, &data).await
                }
                .await;
                emit(
                    &events,
                    Event::Transfer {
                        message: match result {
                            Ok(()) => "Attachment saved".into(),
                            Err(e) => format!("Could not save attachment: {e}"),
                        },
                        busy: false,
                    },
                )
                .await;
            });
        }
    }
    Ok(true)
}
fn text_value(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
