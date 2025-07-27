use aiva_core::{AivaError, Result};
use http_body_util::BodyExt;
use hyper::{Method, Request};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioIo;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::net::UnixStream;
use tracing::{debug, error};

type UnixClient = Client<UnixConnector, String>;

#[derive(Clone)]
struct UnixConnector {
    socket_path: PathBuf,
}

impl tower::Service<hyper::Uri> for UnixConnector {
    type Response = TokioIo<UnixStream>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<Self::Response, Self::Error>>
                + Send,
        >,
    >;

    fn poll_ready(
        &mut self,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: hyper::Uri) -> Self::Future {
        let socket_path = self.socket_path.clone();
        Box::pin(async move {
            let stream = UnixStream::connect(&socket_path)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;
            Ok(TokioIo::new(stream))
        })
    }
}

#[allow(dead_code)]
pub struct FirecrackerApiClient {
    socket_path: PathBuf,
    client: UnixClient,
}

impl FirecrackerApiClient {
    pub fn new(socket_path: PathBuf) -> Result<Self> {
        let connector = UnixConnector {
            socket_path: socket_path.clone(),
        };

        let client = Client::builder(hyper_util::rt::TokioExecutor::new()).build(connector);

        Ok(Self {
            socket_path,
            client,
        })
    }

    async fn make_request<T: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        path: &str,
        body: Option<T>,
    ) -> Result<Option<R>> {
        let uri = format!("http://localhost{path}")
            .parse::<hyper::Uri>()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Invalid URI: {e}"),
                recoverable: false,
            })?;

        let method = method
            .parse::<Method>()
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Invalid HTTP method: {e}"),
                recoverable: false,
            })?;

        let req_builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("Content-Type", "application/json");

        let request = if let Some(body_data) = body {
            let json = serde_json::to_string(&body_data).map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to serialize request body: {e}"),
                recoverable: false,
            })?;
            req_builder
                .body(json)
                .map_err(|e| AivaError::PlatformError {
                    platform: "firecracker".to_string(),
                    message: format!("Failed to build request: {e}"),
                    recoverable: false,
                })?
        } else {
            req_builder
                .body(String::new())
                .map_err(|e| AivaError::PlatformError {
                    platform: "firecracker".to_string(),
                    message: format!("Failed to build request: {e}"),
                    recoverable: false,
                })?
        };

        debug!(
            "Making Firecracker API request: {} {}",
            request.method(),
            request.uri()
        );

        let response =
            self.client
                .request(request)
                .await
                .map_err(|e| AivaError::PlatformError {
                    platform: "firecracker".to_string(),
                    message: format!("Request failed: {e}"),
                    recoverable: true,
                })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .into_body()
                .collect()
                .await
                .map_err(|e| AivaError::PlatformError {
                    platform: "firecracker".to_string(),
                    message: format!("Failed to read error response: {e}"),
                    recoverable: false,
                })?
                .to_bytes();

            let error_text = String::from_utf8_lossy(&body);
            error!("Firecracker API error: {} - {}", status, error_text);

            return Err(AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("API error {status}: {error_text}"),
                recoverable: true,
            });
        }

        let body = response
            .into_body()
            .collect()
            .await
            .map_err(|e| AivaError::PlatformError {
                platform: "firecracker".to_string(),
                message: format!("Failed to read response body: {e}"),
                recoverable: false,
            })?
            .to_bytes();

        if body.is_empty() {
            return Ok(None);
        }

        let result: R = serde_json::from_slice(&body).map_err(|e| AivaError::PlatformError {
            platform: "firecracker".to_string(),
            message: format!("Failed to deserialize response: {e}"),
            recoverable: false,
        })?;

        Ok(Some(result))
    }

    pub async fn configure_machine(&self, vcpu_count: u32, mem_size_mib: u64) -> Result<()> {
        #[derive(Serialize)]
        struct MachineConfig {
            vcpu_count: u32,
            mem_size_mib: u64,
            ht_enabled: bool,
        }

        let config = MachineConfig {
            vcpu_count,
            mem_size_mib,
            ht_enabled: false,
        };

        debug!(
            "Configuring machine: {} vCPUs, {} MiB RAM",
            vcpu_count, mem_size_mib
        );

        self.make_request::<_, serde_json::Value>("PUT", "/machine-config", Some(config))
            .await?;
        Ok(())
    }

    pub async fn configure_boot_source(&self, kernel_path: &Path, boot_args: &str) -> Result<()> {
        #[derive(Serialize)]
        struct BootSource {
            kernel_image_path: String,
            boot_args: String,
        }

        let boot_source = BootSource {
            kernel_image_path: kernel_path.to_string_lossy().to_string(),
            boot_args: boot_args.to_string(),
        };

        debug!("Configuring boot source: {:?}", kernel_path);

        self.make_request::<_, serde_json::Value>("PUT", "/boot-source", Some(boot_source))
            .await?;
        Ok(())
    }

    pub async fn configure_drive(
        &self,
        drive_id: &str,
        path: &Path,
        is_read_only: bool,
        cache_type: &str,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct Drive {
            drive_id: String,
            path_on_host: String,
            is_root_device: bool,
            is_read_only: bool,
            cache_type: String,
        }

        let drive = Drive {
            drive_id: drive_id.to_string(),
            path_on_host: path.to_string_lossy().to_string(),
            is_root_device: drive_id == "rootfs",
            is_read_only,
            cache_type: cache_type.to_string(),
        };

        debug!("Configuring drive {}: {:?}", drive_id, path);

        self.make_request::<_, serde_json::Value>(
            "PUT",
            &format!("/drives/{drive_id}"),
            Some(drive),
        )
        .await?;
        Ok(())
    }

    pub async fn configure_network(
        &self,
        iface_id: &str,
        tap_device: &str,
        _guest_ip: Option<&str>,
    ) -> Result<()> {
        #[derive(Serialize)]
        struct NetworkInterface {
            iface_id: String,
            host_dev_name: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            guest_mac: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            rx_rate_limiter: Option<serde_json::Value>,
            #[serde(skip_serializing_if = "Option::is_none")]
            tx_rate_limiter: Option<serde_json::Value>,
        }

        let network = NetworkInterface {
            iface_id: iface_id.to_string(),
            host_dev_name: tap_device.to_string(),
            guest_mac: None,
            rx_rate_limiter: None,
            tx_rate_limiter: None,
        };

        debug!(
            "Configuring network interface {}: TAP device {}",
            iface_id, tap_device
        );

        self.make_request::<_, serde_json::Value>(
            "PUT",
            &format!("/network-interfaces/{iface_id}"),
            Some(network),
        )
        .await?;
        Ok(())
    }

    pub async fn start_instance(&self) -> Result<()> {
        #[derive(Serialize)]
        struct InstanceStart {
            action_type: String,
        }

        let action = InstanceStart {
            action_type: "InstanceStart".to_string(),
        };

        debug!("Starting Firecracker instance");

        self.make_request::<_, serde_json::Value>("PUT", "/actions", Some(action))
            .await?;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn pause_vm(&self) -> Result<()> {
        #[derive(Serialize)]
        struct VmAction {
            action_type: String,
        }

        let action = VmAction {
            action_type: "Pause".to_string(),
        };

        self.make_request::<_, serde_json::Value>("PATCH", "/vm", Some(action))
            .await?;
        Ok(())
    }

    pub async fn resume_vm(&self) -> Result<()> {
        #[derive(Serialize)]
        struct VmAction {
            action_type: String,
        }

        let action = VmAction {
            action_type: "Resume".to_string(),
        };

        self.make_request::<_, serde_json::Value>("PATCH", "/vm", Some(action))
            .await?;
        Ok(())
    }

    pub async fn shutdown_vm(&self) -> Result<()> {
        #[derive(Serialize)]
        struct InstanceAction {
            action_type: String,
        }

        let action = InstanceAction {
            action_type: "SendCtrlAltDel".to_string(),
        };

        self.make_request::<_, serde_json::Value>("PUT", "/actions", Some(action))
            .await?;
        Ok(())
    }
}
