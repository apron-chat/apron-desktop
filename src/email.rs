//! Dedicated unauthenticated connection: email codes cannot be submitted on guest sessions.
use crate::{auth::Secret, transport::Client};
use serde_json::json;
pub enum Request {
    Propose(String),
    Approve(Secret),
}
pub struct Session {
    pub commands: async_channel::Sender<Request>,
    pub results: async_channel::Receiver<Result<Option<Secret>, String>>,
}
impl Session {
    pub fn start(server: String) -> Self {
        let (tx, rx) = async_channel::bounded(1);
        let (out, input) = async_channel::bounded(1);
        std::thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async move {
                let mut client=None;
                while let Ok(request)=rx.recv().await {
                    let result=async {
                        match request {
                            Request::Propose(address)=> {
                                anyhow::ensure!(address.len()<=320 && address.contains('@') && !address.chars().any(char::is_control),"invalid address");
                                let mut c=Client::connect(&server,None).await?;
                                anyhow::ensure!(c.server["auth"].as_array().is_some_and(|v|v.iter().any(|v|v=="email")),"unsupported email");
                                c.rpc("auth",json!({"scheme":"email","email":address})).await?;
                                client=Some(c);Ok(None)
                            }
                            Request::Approve(code)=> {
                                let c=client.as_mut().ok_or_else(||anyhow::anyhow!("no proposal"))?;
                                let mut result=c.rpc("auth",json!({"scheme":"email","token":code.expose()})).await?;
                                let token=result["token"].take().as_str().map(str::to_owned).ok_or_else(||anyhow::anyhow!("missing token"))?;
                                Ok(Some(Secret::new(token)?))
                            }
                        }
                    }.await;
                    // Never expose provider error bodies, email addresses or entered codes.
                    let done=matches!(result,Ok(Some(_)));
                    if out.send(result.map_err(|_:anyhow::Error|"Email sign-in was refused, expired or interrupted. Check server support and request a new code.".into())).await.is_err() || done {break;}
                }
            });
        });
        Self {
            commands: tx,
            results: input,
        }
    }
}
