use reqwest;
use serde_json::Value;

pub struct ContainerValidator;

impl ContainerValidator {
    pub async fn validate_containers(registry: &str, containers: &[String]) -> Result<(), Box<dyn std::error::Error>> {
        println!("🔍 Validating container images...");
        let mut validation_errors = Vec::new();
        
        for container in containers {
            let parts: Vec<&str> = container.split(':').collect();
            let (image_name, tag) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                (container.as_str(), "latest")
            };
            
            let full_image = format!("{}/{}", registry, image_name);
            
            // Try different validation methods based on registry
            let is_valid = if registry.contains("linuxserver") {
                Self::validate_linuxserver_container(image_name, tag).await
            } else if registry.contains("docker.io") || registry.contains("hub.docker.com") {
                Self::validate_dockerhub_container(image_name, tag).await
            } else {
                // For other registries, try a generic approach or skip validation
                Self::validate_generic_container(&full_image, tag).await
            };
            
            if !is_valid {
                validation_errors.push(format!("Container '{}:{}' not found in registry '{}'", image_name, tag, registry));
            } else {
                println!("  ✓ {}/{}", registry, container);
            }
        }
        
        if validation_errors.is_empty() {
            println!("✅ All containers validated successfully");
            Ok(())
        } else {
            Err(format!("Container validation failed:\n{}", validation_errors.join("\n")).into())
        }
    }

    async fn validate_linuxserver_container(image_name: &str, _tag: &str) -> bool {
        let client = reqwest::Client::new();
        match client
            .get("https://api.linuxserver.io/api/v1/images?include_config=false&include_deprecated=false")
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(parsed) = response.json::<Value>().await {
                        if let Some(repositories) = parsed["data"]["repositories"]["linuxserver"].as_array() {
                            return repositories.iter().any(|repo| {
                                repo["name"].as_str() == Some(image_name) && 
                                !repo["deprecated"].as_bool().unwrap_or(false)
                            });
                        }
                    }
                }
            },
            Err(_) => {}
        }
        false
    }

    async fn validate_dockerhub_container(image_name: &str, tag: &str) -> bool {
        let client = reqwest::Client::new();
        let url = format!("https://hub.docker.com/v2/repositories/{}/tags/{}", image_name, tag);
        
        match client.get(&url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    async fn validate_generic_container(full_image: &str, _tag: &str) -> bool {
        // For generic registries, we could try registry API v2
        // For now, return true to avoid blocking unknown registries
        // In production, you might want to implement registry-specific validation
        println!("  ? Skipping validation for unknown registry: {}", full_image);
        true
    }

    pub async fn get_available_linuxserver_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
        println!("🔍 Fetching available LinuxServer containers...");
        
        let client = reqwest::Client::new();
        match client
            .get("https://api.linuxserver.io/api/v1/images?include_config=false&include_deprecated=false")
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(parsed) = response.json::<Value>().await {
                        if let Some(repositories) = parsed["data"]["repositories"]["linuxserver"].as_array() {
                            let containers: Vec<String> = repositories
                                .iter()
                                .filter_map(|repo| {
                                    let name = repo["name"].as_str()?;
                                    let deprecated = repo["deprecated"].as_bool().unwrap_or(false);
                                    let stable = repo["stable"].as_bool().unwrap_or(true);
                                    
                                    if !deprecated && stable {
                                        Some(format!("{}:latest", name))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            
                            println!("✅ Found {} available containers", containers.len());
                            return Ok(containers);
                        }
                    }
                }
            },
            Err(e) => {
                return Err(format!("Failed to fetch containers: {}", e).into());
            }
        }
        
        Err("Failed to parse LinuxServer API response".into())
    }
}