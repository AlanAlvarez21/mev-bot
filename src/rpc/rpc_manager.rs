use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use reqwest::Client;
use serde_json::{json, Value};
use tokio::sync::RwLock;
use crate::logging::Logger;

#[derive(Debug, Clone)]
pub enum RpcTaskType {
    Read,      // getAccountInfo, getMultipleAccounts, getBlock, etc.
    Simulate,  // simulateTransaction
    Execute,   // sendTransaction, sendBundle via Jito
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RpcEndpointType {
    Helius,
    Jito,
    Drpc,
}

#[derive(Debug, Clone)]
pub struct RpcHealthStatus {
    pub latency_ms: f64,
    pub success_rate: f64,
    pub last_check: Instant,
    pub is_healthy: bool,
}

#[derive(Debug, Clone)]
pub struct RpcEndpoint {
    pub url: String,
    pub endpoint_type: RpcEndpointType,
    pub health: RpcHealthStatus,
    pub weight: f64,  // For load balancing, higher weight = more requests
}

#[derive(Debug)]
pub struct RpcManager {
    client: Arc<Client>,
    endpoints: Arc<RwLock<HashMap<RpcEndpointType, RpcEndpoint>>>,
    health_check_interval: Duration,
}

impl RpcManager {
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(Client::new());
        let endpoints = Arc::new(RwLock::new(HashMap::new()));
        
        let mut rpc_manager = Self {
            client,
            endpoints,
            health_check_interval: Duration::from_secs(30), // Check every 30 seconds
        };
        
        // Initialize endpoints from environment variables
        rpc_manager.load_endpoints_from_env().await?;
        
        // Start health checks
        rpc_manager.start_health_checks().await;
        
        Ok(rpc_manager)
    }
    
    async fn load_endpoints_from_env(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut endpoints = self.endpoints.write().await;
        
        // Load HELIUS endpoint
        let helius_url = std::env::var("HELIUS")
            .map_err(|_| "HELIUS environment variable not set")?;
        endpoints.insert(
            RpcEndpointType::Helius,
            RpcEndpoint {
                url: helius_url,
                endpoint_type: RpcEndpointType::Helius,
                health: RpcHealthStatus {
                    latency_ms: 0.0,
                    success_rate: 1.0,
                    last_check: Instant::now(),
                    is_healthy: false,
                },
                weight: 1.0,
            }
        );
        
        // Load JITO RPC endpoint
        let jito_url = std::env::var("JITO_RPC_URL")
            .map_err(|_| "JITO_RPC_URL environment variable not set")?;
        endpoints.insert(
            RpcEndpointType::Jito,
            RpcEndpoint {
                url: jito_url,
                endpoint_type: RpcEndpointType::Jito,
                health: RpcHealthStatus {
                    latency_ms: 0.0,
                    success_rate: 1.0,
                    last_check: Instant::now(),
                    is_healthy: false,
                },
                weight: 1.0,
            }
        );
        
        // Load DRPC endpoint
        let drpc_url = std::env::var("DRPC")
            .map_err(|_| "DRPC environment variable not set")?;
        endpoints.insert(
            RpcEndpointType::Drpc,
            RpcEndpoint {
                url: drpc_url,
                endpoint_type: RpcEndpointType::Drpc,
                health: RpcHealthStatus {
                    latency_ms: 0.0,
                    success_rate: 1.0,
                    last_check: Instant::now(),
                    is_healthy: false,
                },
                weight: 0.5, // Lower weight as fallback
            }
        );
        
        Ok(())
    }
    
    pub async fn get_best_rpc(&self, task_type: RpcTaskType) -> Option<RpcEndpoint> {
        let endpoints = self.endpoints.read().await;
        
        match task_type {
            RpcTaskType::Read | RpcTaskType::Simulate => {
                // Prefer HELIUS for reads and simulations
                if let Some(helius) = endpoints.get(&RpcEndpointType::Helius) {
                    if helius.health.is_healthy {
                        return Some(helius.clone());
                    }
                }
                
                // Fallback to DRPC for reads/simulations
                if let Some(drpc) = endpoints.get(&RpcEndpointType::Drpc) {
                    if drpc.health.is_healthy {
                        return Some(drpc.clone());
                    }
                }
                
                // If HELIUS is down, try JITO as last resort for reads
                if let Some(jito) = endpoints.get(&RpcEndpointType::Jito) {
                    if jito.health.is_healthy {
                        return Some(jito.clone());
                    }
                }
            },
            RpcTaskType::Execute => {
                // Prefer JITO for execution
                if let Some(jito) = endpoints.get(&RpcEndpointType::Jito) {
                    if jito.health.is_healthy {
                        return Some(jito.clone());
                    }
                }
                
                // Fallback to DRPC for execution only if JITO unavailable
                if let Some(drpc) = endpoints.get(&RpcEndpointType::Drpc) {
                    if drpc.health.is_healthy {
                        return Some(drpc.clone());
                    }
                }
            },
        }
        
        None
    }
    
    pub async fn make_request(&self, endpoint_type: RpcEndpointType, request_body: Value) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let endpoint = {
            let endpoints = self.endpoints.read().await;
            match endpoints.get(&endpoint_type) {
                Some(ep) => ep.clone(),
                None => return Err(format!("RPC endpoint {:?} not configured", endpoint_type).into()),
            }
        };
        
        let start_time = Instant::now();
        
        let response = self.client
            .post(&endpoint.url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;
        
        let elapsed = start_time.elapsed().as_millis() as f64;
        
        let response_text = response.text().await
            .map_err(|e| format!("Failed to read response: {}", e))?;
        
        let response_value: Value = serde_json::from_str(&response_text)
            .map_err(|e| format!("Failed to parse response as JSON: {}", e))?;
        
        // Update health metrics based on success
        self.update_health(endpoint_type, elapsed, true).await;
        
        Ok(response_value)
    }
    
    pub async fn health_check(&self, endpoint_type: RpcEndpointType) -> Result<RpcHealthStatus, Box<dyn std::error::Error + Send + Sync>> {
        let start_time = Instant::now();
        
        let test_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getHealth",
            "params": []
        });
        
        let success = match self.make_request(endpoint_type.clone(), test_request).await {
            Ok(response) => {
                // Check if response indicates healthy status
                response["result"]["health"].as_str().map(|s| s == "ok").unwrap_or(false)
            },
            Err(_) => false,
        };
        
        let latency = start_time.elapsed().as_millis() as f64;
        
        Ok(RpcHealthStatus {
            latency_ms: latency,
            success_rate: if success { 1.0 } else { 0.0 },
            last_check: Instant::now(),
            is_healthy: success,
        })
    }
    
    async fn update_health(&self, endpoint_type: RpcEndpointType, latency_ms: f64, success: bool) {
        let mut endpoints = self.endpoints.write().await;
        
        if let Some(endpoint) = endpoints.get_mut(&endpoint_type) {
            // Update rolling average for success rate (simple exponentially weighted)
            endpoint.health.success_rate = 0.9 * endpoint.health.success_rate + 0.1 * if success { 1.0 } else { 0.0 };
            
            // Update latency (simple average of recent measurements)
            endpoint.health.latency_ms = (endpoint.health.latency_ms + latency_ms) / 2.0;
            endpoint.health.last_check = Instant::now();
            endpoint.health.is_healthy = success && latency_ms < 2000.0; // Healthy if under 2s latency
        }
    }
    
    async fn start_health_checks(&self) {
        let self_clone = self.clone_for_spawn();
        
        tokio::spawn(async move {
            loop {
                self_clone.run_health_checks().await;
                tokio::time::sleep(Duration::from_secs(30)).await; // Check every 30 seconds
            }
        });
    }
    
    async fn run_health_checks(&self) {
        let endpoint_types = vec![
            RpcEndpointType::Helius,
            RpcEndpointType::Jito,
            RpcEndpointType::Drpc,
        ];
        
        for endpoint_type in endpoint_types {
            match self.health_check(endpoint_type.clone()).await {
                Ok(health_status) => {
                    let mut endpoints = self.endpoints.write().await;
                    if let Some(endpoint) = endpoints.get_mut(&endpoint_type) {
                        endpoint.health = health_status;
                    }
                },
                Err(e) => {
                    Logger::error_occurred(&format!("Health check failed for {:?}: {}", endpoint_type, e));
                    
                    // Mark as unhealthy
                    let mut endpoints = self.endpoints.write().await;
                    if let Some(endpoint) = endpoints.get_mut(&endpoint_type) {
                        endpoint.health.is_healthy = false;
                        endpoint.health.last_check = Instant::now();
                    }
                }
            }
        }
    }
    
    // Helper for spawning async tasks
    fn clone_for_spawn(&self) -> RpcManager {
        RpcManager {
            client: Arc::clone(&self.client),
            endpoints: Arc::clone(&self.endpoints),
            health_check_interval: self.health_check_interval,
        }
    }
    
    // Convenience methods for specific RPC calls
    pub async fn get_account_info(&self, account: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                account,
                {
                    "encoding": "jsonParsed"
                }
            ]
        });
        
        let endpoint = self.get_best_rpc(RpcTaskType::Read).await
            .ok_or("No healthy read endpoint available")?;
        
        let response = self.make_request(endpoint.endpoint_type, request_body).await?;
        
        if let Some(error) = response.get("error") {
            return Err(format!("getAccountInfo failed: {}", error).into());
        }
        
        Ok(response)
    }
    
    pub async fn simulate_transaction(&self, transaction_data: &str) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "simulateTransaction",
            "params": [
                transaction_data,
                {
                    "encoding": "base64",
                    "sigVerify": false,
                    "replaceRecentBlockhash": true
                }
            ]
        });
        
        let endpoint = self.get_best_rpc(RpcTaskType::Simulate).await
            .ok_or("No healthy simulation endpoint available")?;
        
        let response = self.make_request(endpoint.endpoint_type, request_body).await?;
        
        if let Some(error) = response.get("error") {
            return Err(format!("simulateTransaction failed: {}", error).into());
        }
        
        Ok(response)
    }
    
    pub async fn get_recent_blockhash(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestBlockhash",
            "params": []
        });
        
        let endpoint = self.get_best_rpc(RpcTaskType::Read).await
            .ok_or("No healthy read endpoint available")?;
        
        let response = self.make_request(endpoint.endpoint_type, request_body).await?;
        
        if let Some(error) = response.get("error") {
            return Err(format!("getLatestBlockhash failed: {}", error).into());
        }
        
        Ok(response)
    }
    
    pub async fn get_recent_prioritization_fees(&self) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let request_body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getRecentPrioritizationFees",
            "params": []
        });
        
        let endpoint = self.get_best_rpc(RpcTaskType::Read).await
            .ok_or("No healthy read endpoint available")?;
        
        let response = self.make_request(endpoint.endpoint_type, request_body).await?;
        
        if let Some(error) = response.get("error") {
            return Err(format!("getRecentPrioritizationFees failed: {}", error).into());
        }
        
        Ok(response)
    }
}

impl Clone for RpcManager {
    fn clone(&self) -> Self {
        RpcManager {
            client: Arc::clone(&self.client),
            endpoints: Arc::clone(&self.endpoints),
            health_check_interval: self.health_check_interval,
        }
    }
}