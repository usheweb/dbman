use std::sync::Arc;
use std::time::Duration;

use russh::client;
use russh::keys::key::PublicKey;

use crate::models::SshConfig;

/// 接受任意主机公钥：行为对齐 server.js（ssh2 未做 host key 校验）。
struct AcceptAnyKey;

#[async_trait::async_trait]
impl client::Handler for AcceptAnyKey {
    type Error = russh::Error;

    async fn check_server_key(&mut self, _server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(true)
    }
}

/// 一条本地监听 → 远端数据库的 SSH 隧道。Tunnel 被 drop 时自动中止后台任务，
/// 从而关闭 SSH 会话。
pub struct Tunnel {
    abort: tokio::task::AbortHandle,
    pub local_port: u16,
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

/// 建立 SSH 隧道：本地随机端口 → (remote_host, remote_port)。
pub async fn connect_tunnel(
    ssh: &SshConfig,
    remote_host: &str,
    remote_port: u16,
) -> Result<Tunnel, String> {
    let ssh_host = ssh.host.clone().unwrap_or_default();
    let ssh_port = ssh.port.unwrap_or(22);

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(3600)),
        ..Default::default()
    });

    let mut session = client::connect(config, (ssh_host.as_str(), ssh_port), AcceptAnyKey)
        .await
        .map_err(|e| e.to_string())?;

    let username = ssh.username.clone().unwrap_or_default();
    let password = ssh.password.clone().filter(|p| !p.is_empty());
    let private_key = ssh.private_key.clone().filter(|k| !k.is_empty());

    let authenticated = if let Some(password) = password {
        session
            .authenticate_password(username, password)
            .await
            .map_err(|e| e.to_string())?
    } else if let Some(pem) = private_key {
        let key = russh::keys::decode_secret_key(&pem, ssh.password.as_deref())
            .map_err(|e| e.to_string())?;
        session
            .authenticate_publickey(username, Arc::new(key))
            .await
            .map_err(|e| e.to_string())?
    } else {
        return Err("No SSH authentication method provided".to_string());
    };

    if !authenticated {
        return Err("SSH authentication failed".to_string());
    }

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| e.to_string())?;
    let local_port = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();

    let remote_host = remote_host.to_string();
    let join = tokio::spawn(async move {
        loop {
            let (socket, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };
            match session
                .channel_open_direct_tcpip(
                    remote_host.clone(),
                    remote_port as u32,
                    "127.0.0.1",
                    local_port as u32,
                )
                .await
            {
                Ok(channel) => {
                    tokio::spawn(async move {
                        let mut socket = socket;
                        let mut stream = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                    });
                }
                Err(_) => {
                    // 打开通道失败，丢弃该连接
                }
            }
        }
    });

    Ok(Tunnel {
        abort: join.abort_handle(),
        local_port,
    })
}
