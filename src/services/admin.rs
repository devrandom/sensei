use super::{PaginationRequest, PaginationResponse};
use crate::error::Error as SenseiError;
use crate::{
    config::LightningNodeBackendConfig,
    database::{
        self,
        admin::{AdminDatabase, Node, Role, Status},
    },
};
use crate::{
    config::{LightningNodeConfig, SenseiConfig},
    hex_utils,
    node::LightningNode,
    NodeDirectory, NodeHandle,
};

use serde::Serialize;
use std::{collections::hash_map::Entry, fs, sync::Arc};
use tokio::sync::Mutex;
pub enum AdminRequest {
    GetConfig {},
    UpdateConfig {
        electrum_url: String,
    },
    GetStatus {
        pubkey: String,
    },
    CreateAdmin {
        username: String,
        alias: String,
        passphrase: String,
        electrum_url: String,
        start: bool,
    },
    StartAdmin {
        passphrase: String,
    },
    CreateNode {
        username: String,
        alias: String,
        passphrase: String,
        start: bool,
    },
    ListNodes {
        pagination: PaginationRequest,
    },
    DeleteNode {
        pubkey: String,
    },
    StartNode {
        pubkey: String,
        passphrase: String,
    },
    StopNode {
        pubkey: String,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum AdminResponse {
    GetConfig {
        electrum_url: String,
    },
    UpdateConfig {},
    GetStatus {
        alias: Option<String>,
        created: bool,
        running: bool,
        authenticated: bool,
        pubkey: Option<String>,
        username: Option<String>,
        role: Option<u8>,
    },
    CreateAdmin {
        pubkey: String,
        macaroon: String,
        external_id: String,
        role: u8,
    },
    StartAdmin {
        pubkey: String,
        macaroon: String,
    },
    CreateNode {
        pubkey: String,
        macaroon: String,
    },
    ListNodes {
        nodes: Vec<Node>,
        pagination: PaginationResponse,
    },
    DeleteNode {},
    StartNode {
        macaroon: String,
    },
    StopNode {},
    Error(Error),
}

#[derive(Clone)]
pub struct AdminService {
    pub data_dir: String,
    pub config: Arc<Mutex<SenseiConfig>>,
    pub node_directory: NodeDirectory,
    pub database: Arc<Mutex<AdminDatabase>>,
}

impl AdminService {
    pub fn new(
        data_dir: &str,
        config: SenseiConfig,
        node_directory: NodeDirectory,
        database: AdminDatabase,
    ) -> Self {
        Self {
            data_dir: String::from(data_dir),
            config: Arc::new(Mutex::new(config)),
            node_directory,
            database: Arc::new(Mutex::new(database)),
        }
    }
}

#[derive(Serialize, Debug)]
pub enum Error {
    Generic(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Generic(e.to_string())
    }
}

impl From<SenseiError> for Error {
    fn from(e: SenseiError) -> Self {
        Self::Generic(e.to_string())
    }
}

impl From<database::Error> for Error {
    fn from(e: database::Error) -> Self {
        match e {
            database::Error::Generic(str) => Self::Generic(str),
        }
    }
}

impl From<macaroon::MacaroonError> for Error {
    fn from(_e: macaroon::MacaroonError) -> Self {
        Self::Generic(String::from("macaroon error"))
    }
}

impl AdminService {
    pub async fn call(&self, request: AdminRequest) -> Result<AdminResponse, Error> {
        match request {
            AdminRequest::GetConfig {} => {
                let config = self.config.lock().await;
                let electrum_url = match &config.backend {
                    LightningNodeBackendConfig::Electrum(electrum_config) => {
                        electrum_config.url.clone()
                    }
                };
                Ok(AdminResponse::GetConfig { electrum_url })
            }
            AdminRequest::UpdateConfig { electrum_url } => {
                let new_backend_config = {
                    let config = self.config.lock().await;
                    config.backend.clone_with_new_url(electrum_url)
                };
                {
                    let mut config = self.config.lock().await;
                    config.set_backend(new_backend_config);
                    config.save();
                }
                Ok(AdminResponse::UpdateConfig {})
            }
            AdminRequest::GetStatus { pubkey } => {
                let mut database = self.database.lock().await;
                let admin_node = database.get_admin_node()?;
                let created = admin_node.is_some();
                let node = database.get_node_by_pubkey(&pubkey)?;
                match node {
                    Some(node) => {
                        let directory = self.node_directory.lock().await;
                        let node_running = directory.contains_key(&node.pubkey);
                        Ok(AdminResponse::GetStatus {
                            alias: Some(node.alias),
                            created,
                            running: node_running,
                            authenticated: pubkey == node.pubkey,
                            pubkey: Some(pubkey),
                            username: Some(node.username),
                            role: Some(node.role),
                        })
                    }
                    None => Ok(AdminResponse::GetStatus {
                        alias: None,
                        pubkey: Some(pubkey),
                        created,
                        running: false,
                        authenticated: false,
                        username: None,
                        role: None,
                    }),
                }
            }
            AdminRequest::CreateAdmin {
                username,
                alias,
                passphrase,
                electrum_url,
                start,
            } => {
                let new_backend_config = {
                    let config = self.config.lock().await;
                    config.backend.clone_with_new_url(electrum_url)
                };
                {
                    let mut config = self.config.lock().await;
                    config.set_backend(new_backend_config);
                    config.save();
                }

                let (lightning_node, node) = self
                    .create_node(username, alias, passphrase.clone(), Role::Admin)
                    .await?;
                let node_info = lightning_node.node_info()?;
                let macaroon = lightning_node.macaroon.serialize(macaroon::Format::V2)?;

                if start {
                    self.start_node(node.clone(), passphrase).await?;
                }
                Ok(AdminResponse::CreateAdmin {
                    pubkey: node_info.node_pubkey,
                    macaroon: hex_utils::hex_str(macaroon.as_slice()),
                    external_id: node.external_id,
                    role: node.role,
                })
            }
            AdminRequest::StartAdmin { passphrase } => {
                let db_node_result = {
                    let mut database = self.database.lock().await;
                    database.get_admin_node()
                };

                let db_node = db_node_result?;

                match db_node {
                    Some(node) => {
                        let lightning_node = self.start_node(node.clone(), passphrase).await?;
                        let node_info = lightning_node.node_info()?;
                        let macaroon = lightning_node.macaroon.serialize(macaroon::Format::V2)?;
                        Ok(AdminResponse::StartAdmin {
                            pubkey: node_info.node_pubkey,
                            macaroon: hex_utils::hex_str(macaroon.as_slice()),
                        })
                    }
                    None => Err(Error::Generic(String::from(
                        "admin node not found, run create first",
                    ))),
                }
            }
            AdminRequest::StartNode { pubkey, passphrase } => {
                let db_node = {
                    let mut database = self.database.lock().await;
                    database.get_node_by_pubkey(&pubkey)?
                };
                match db_node {
                    Some(node) => {
                        let lightning_node = self.start_node(node.clone(), passphrase).await?;
                        let macaroon = lightning_node.macaroon.serialize(macaroon::Format::V2)?;
                        Ok(AdminResponse::StartNode {
                            macaroon: hex_utils::hex_str(macaroon.as_slice()),
                        })
                    }
                    None => Err(Error::Generic(String::from("node not found"))),
                }
            }
            AdminRequest::StopNode { pubkey } => {
                let db_node = {
                    let mut database = self.database.lock().await;
                    database.get_node_by_pubkey(&pubkey)?
                };
                match db_node {
                    Some(mut node) => {
                        self.stop_node(pubkey).await?;
                        {
                            node.status = Status::Stopped.to_integer();
                            let mut database = self.database.lock().await;
                            database.update_node(node)?;
                        }
                        Ok(AdminResponse::StopNode {})
                    }
                    None => {
                        // try stopping it anyway?
                        Ok(AdminResponse::StopNode {})
                    }
                }
            }
            AdminRequest::CreateNode {
                username,
                alias,
                passphrase,
                start,
            } => {
                let (lightning_node, node) = self
                    .create_node(username, alias, passphrase.clone(), Role::User)
                    .await?;
                let node_info = lightning_node.node_info()?;
                let macaroon = lightning_node.macaroon.serialize(macaroon::Format::V2)?;

                if start {
                    self.start_node(node.clone(), passphrase).await?;
                }
                Ok(AdminResponse::CreateNode {
                    pubkey: node_info.node_pubkey,
                    macaroon: hex_utils::hex_str(macaroon.as_slice()),
                })
            }
            AdminRequest::ListNodes { pagination } => {
                let (nodes, pagination) = self.list_nodes(pagination).await?;
                Ok(AdminResponse::ListNodes { nodes, pagination })
            }
            AdminRequest::DeleteNode { pubkey } => {
                let mut database = self.database.lock().await;
                let db_node = database.get_node_by_pubkey(&pubkey)?;
                match db_node {
                    Some(node) => {
                        self.delete_node(node).await?;
                        Ok(AdminResponse::DeleteNode {})
                    }
                    None => Err(Error::Generic(String::from("node not found"))),
                }
            }
        }
    }

    pub async fn get_node_details(
        &self,
        pubkey: String,
    ) -> Result<Option<Node>, crate::error::Error> {
        let mut database = self.database.lock().await;
        let node = database.get_node_by_pubkey(&pubkey)?;
        Ok(node)
    }

    async fn list_nodes(
        &self,
        pagination: PaginationRequest,
    ) -> Result<(Vec<Node>, PaginationResponse), crate::error::Error> {
        let mut database = self.database.lock().await;
        Ok(database.list_nodes(pagination)?)
    }

    async fn create_node(
        &self,
        username: String,
        alias: String,
        passphrase: String,
        role: Role,
    ) -> Result<(LightningNode, Node), crate::error::Error> {
        let network = { self.config.lock().await.network };
        let listen_addr = public_ip::addr().await.unwrap().to_string();
        let listen_port = {
            let mut database = self.database.lock().await;
            let mut port = portpicker::pick_unused_port().expect("no ports left");
            let mut port_in_use = database.port_in_use(port)?;

            while port_in_use {
                port = portpicker::pick_unused_port().expect("no ports left");
                port_in_use = database.port_in_use(port)?;
            }

            port
        };

        let mut node = {
            let mut node = match role {
                Role::Admin => Node::new_admin(
                    username,
                    alias,
                    network.to_string(),
                    listen_addr,
                    listen_port,
                ),
                Role::User => Node::new(
                    username,
                    alias,
                    network.to_string(),
                    listen_addr,
                    listen_port,
                ),
            };
            let mut database = self.database.lock().await;
            node.id = database.create_node(node.clone())?;
            node
        };

        let lightning_node = self.get_node(node.clone(), passphrase).await?;
        node.pubkey = lightning_node.node_info()?.node_pubkey;

        {
            let mut database = self.database.lock().await;
            database.update_node(node.clone())?;
        }

        Ok((lightning_node, node))
    }

    pub async fn get_node(
        &self,
        node: Node,
        passphrase: String,
    ) -> Result<LightningNode, crate::error::Error> {
        let node_config = self.get_node_config(node.clone(), passphrase).await;
        if node.is_user() {
            let mut database = self.database.lock().await;
            let admin_node_db = database.get_admin_node()?;
            match admin_node_db {
                Some(admin_node_db) => {
                    let mut node_directory = self.node_directory.lock().await;
                    let admin_node_entry = node_directory.entry(admin_node_db.pubkey.clone());
                    match admin_node_entry {
                        Entry::Occupied(entry) => {
                            let admin_node_handle = entry.get();
                            let network_graph = admin_node_handle.node.network_graph.clone();
                            let network_graph_message_handler =
                                admin_node_handle.node.network_graph_msg_handler.clone();
                            LightningNode::new(
                                node_config,
                                Some(network_graph),
                                Some(network_graph_message_handler),
                            )
                        }
                        Entry::Vacant(_entry) => Err(crate::error::Error::AdminNodeNotStarted),
                    }
                }
                None => Err(crate::error::Error::AdminNodeNotCreated),
            }
        } else {
            LightningNode::new(node_config, None, None)
        }
    }

    async fn get_node_config(&self, node: Node, passphrase: String) -> LightningNodeConfig {
        let external_router = node.is_user();
        let config = self.config.lock().await;
        LightningNodeConfig {
            backend: config.backend.clone(),
            data_dir: format!("{}/{}/{}", self.data_dir, config.network, node.external_id),
            ldk_peer_listening_port: node.listen_port,
            ldk_announced_listen_addr: vec![],
            ldk_announced_node_name: Some(node.alias),
            network: config.network,
            passphrase,
            external_router,
        }
    }

    // note: please be sure to stop the node first? maybe?
    async fn delete_node(&self, node: Node) -> Result<(), crate::error::Error> {
        let node_config = self.get_node_config(node, "".into()).await;
        Ok(fs::remove_dir_all(&node_config.data_dir)?)
    }

    async fn start_node(
        &self,
        mut node: Node,
        passphrase: String,
    ) -> Result<LightningNode, crate::error::Error> {
        let lightning_node = self.get_node(node.clone(), passphrase).await?;
        let mut node_directory = self.node_directory.lock().await;
        let entry = node_directory.entry(node.pubkey.clone());

        if let Entry::Vacant(entry) = entry {
            let start_lightning_node = lightning_node.clone();
            println!(
                "starting node {} on port {}",
                node.pubkey.clone(),
                node.listen_port
            );
            let (handles, background_processor) = start_lightning_node.start();
            entry.insert(NodeHandle {
                node: Arc::new(lightning_node.clone()),
                background_processor,
                handles,
            });

            node.status = Status::Running.to_integer();
            node.listen_addr = public_ip::addr().await.unwrap().to_string();
            let mut database = self.database.lock().await;
            database.update_node(node)?;
        }
        Ok(lightning_node)
    }

    async fn stop_node(&self, pubkey: String) -> Result<(), crate::error::Error> {
        let mut node_directory = self.node_directory.lock().await;
        let entry = node_directory.entry(pubkey.clone());

        if let Entry::Occupied(entry) = entry {
            let node_handle = entry.remove();
            // TODO: stop accepting new incoming connections somehow?
            node_handle.node.peer_manager.disconnect_all_peers();
            let _res = node_handle.background_processor.stop();
            for handle in node_handle.handles {
                handle.abort();
            }
        }

        Ok(())
    }
}