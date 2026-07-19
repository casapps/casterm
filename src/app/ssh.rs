//! SSH connection manager

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::support::error::{CastermError, Result};

static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

/// Unique connection identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConnectionId(u64);

impl ConnectionId {
    pub fn next() -> Self {
        Self(NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl std::fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ssh-{}", self.0)
    }
}

/// SSH authentication method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    /// Password authentication
    Password(String),
    /// Public key from file
    PublicKey(PathBuf),
    /// SSH agent forwarding
    Agent,
    /// Keyboard-interactive
    KeyboardInteractive,
    /// GSSAPI/Kerberos
    Gssapi,
}

/// SSH jump host configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpHost {
    /// Jump host address
    pub host: String,
    /// Jump host port
    pub port: u16,
    /// Username on the jump host
    pub username: String,
    /// Authentication for the jump host
    pub auth: AuthMethod,
}

/// Port forwarding rule type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ForwardType {
    /// Local port forwarding (local:remote)
    Local {
        local_addr: SocketAddr,
        remote_host: String,
        remote_port: u16,
    },
    /// Remote port forwarding (remote:local)
    Remote {
        remote_addr: SocketAddr,
        local_host: String,
        local_port: u16,
    },
    /// Dynamic SOCKS proxy
    Dynamic { bind_addr: SocketAddr },
}

/// SSH connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    /// Connection identifier
    pub id: ConnectionId,
    /// Display name for this connection
    pub name: String,
    /// Remote hostname or IP address
    pub host: String,
    /// Remote port (default 22)
    pub port: u16,
    /// Username on remote host
    pub username: String,
    /// Authentication method
    pub auth: AuthMethod,
    /// Jump host chain (traversed in order)
    pub jump_hosts: Vec<JumpHost>,
    /// Port forwarding rules
    pub forwards: Vec<ForwardType>,
    /// Enable X11 forwarding
    pub x11_forwarding: bool,
    /// Enable agent forwarding
    pub agent_forwarding: bool,
    /// Keepalive interval in seconds (0 = disabled)
    pub keepalive_interval: u32,
    /// Connection timeout in seconds
    pub timeout: u32,
    /// Number of reconnect attempts
    pub reconnect_attempts: u32,
    /// Seconds between reconnect attempts
    pub reconnect_delay: u32,
    /// Optional profile override name
    pub profile: Option<String>,
    /// Optional color label for visual identification
    pub color: Option<String>,
}

impl Default for SshConfig {
    fn default() -> Self {
        Self {
            id: ConnectionId::next(),
            name: String::new(),
            host: String::new(),
            port: 22,
            username: whoami_username(),
            auth: AuthMethod::Agent,
            jump_hosts: Vec::new(),
            forwards: Vec::new(),
            x11_forwarding: false,
            agent_forwarding: true,
            keepalive_interval: 60,
            timeout: 30,
            reconnect_attempts: 3,
            reconnect_delay: 5,
            profile: None,
            color: None,
        }
    }
}

/// SSH connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
    Failed,
}

/// An SSH connection entry in the connection manager
pub struct SshConnection {
    pub config: SshConfig,
    pub state: ConnectionState,
    /// Error message if state is Failed
    pub error: Option<String>,
}

impl SshConnection {
    pub fn new(config: SshConfig) -> Self {
        Self {
            config,
            state: ConnectionState::Disconnected,
            error: None,
        }
    }
}

/// SSH connection manager — maintains the host directory and connection state
pub struct SshManager {
    connections: Vec<SshConnection>,
}

impl SshManager {
    pub fn new() -> Self {
        Self {
            connections: Vec::new(),
        }
    }

    /// Add a new connection to the host directory
    pub fn add(&mut self, config: SshConfig) -> ConnectionId {
        let id = config.id;
        self.connections.push(SshConnection::new(config));
        id
    }

    /// Remove a connection from the host directory
    pub fn remove(&mut self, id: ConnectionId) {
        self.connections.retain(|c| c.config.id != id);
    }

    /// Get all connections
    pub fn list(&self) -> &[SshConnection] {
        &self.connections
    }

    /// Find a connection by ID
    pub fn get(&self, id: ConnectionId) -> Option<&SshConnection> {
        self.connections.iter().find(|c| c.config.id == id)
    }

    /// Find a connection by ID (mutable)
    pub fn get_mut(&mut self, id: ConnectionId) -> Option<&mut SshConnection> {
        self.connections.iter_mut().find(|c| c.config.id == id)
    }

    /// Find a connection by display name
    pub fn find_by_name(&self, name: &str) -> Option<&SshConnection> {
        self.connections.iter().find(|c| c.config.name == name)
    }

    /// Validate a config — returns Ok(()) if all required fields are present
    pub fn validate(config: &SshConfig) -> Result<()> {
        if config.host.is_empty() {
            return Err(CastermError::Config("SSH host cannot be empty".into()));
        }
        if config.username.is_empty() {
            return Err(CastermError::Config("SSH username cannot be empty".into()));
        }
        if config.port == 0 {
            return Err(CastermError::Config("SSH port must be non-zero".into()));
        }
        Ok(())
    }

    /// Build a connection URL string for display
    pub fn connection_url(config: &SshConfig) -> String {
        if config.port == 22 {
            format!("{}@{}", config.username, config.host)
        } else {
            format!("{}@{}:{}", config.username, config.host, config.port)
        }
    }
}

/// Get the current username for SSH default
fn whoami_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".to_string())
}
