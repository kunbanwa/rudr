use k8s_openapi::api::apps::v1 as apps;
use k8s_openapi::api::core::v1 as api;
use k8s_openapi::apimachinery::pkg::apis::meta::v1 as meta;
use kube::api::PostParams;
use kube::client::APIClient;

use crate::schematic::component::Component;
use crate::workload_type::{InstigatorResult, KubeName, ParamMap, WorkloadType};

use serde_json::to_string_pretty as to_json;
use std::collections::BTreeMap;

/// A Replicated Service can take one component and scale it up or down.
pub struct ReplicatedService {
    pub name: String,
    pub namespace: String,
    pub component_name: String,
    pub instance_name: String,
    pub definition: Component,
    pub client: APIClient,
    pub params: ParamMap,
    pub owner_ref: Option<Vec<meta::OwnerReference>>,
}

impl ReplicatedService {
    /// Create a Pod definition that describes this Singleton
    fn to_deployment(&self) -> apps::Deployment {
        let mut labels = BTreeMap::new();
        labels.insert("app".to_string(), self.name.clone());
        labels.insert(
            "workload-type".to_string(),
            "replicated-service".to_string(),
        );

        apps::Deployment {
            // TODO: Could make this generic.
            metadata: Some(meta::ObjectMeta {
                name: Some(self.kube_name()),
                labels: Some(labels),
                owner_references: self.owner_ref.clone(),
                ..Default::default()
            }),
            spec: Some(self.definition.to_deployment_spec(self.name.clone())),
            ..Default::default()
        }
    }
    /// Create a service if this component has a port.
    fn to_service(&self) -> Option<api::Service> {
        self.definition.listening_port().and_then(|port| {
            let mut labels = BTreeMap::new();
            labels.insert("component".to_string(), self.name.clone());
            labels.insert(
                "workload-type".to_string(),
                "replicated-service".to_string(),
            );

            Some(api::Service {
                metadata: Some(meta::ObjectMeta {
                    name: Some(self.kube_name()),
                    labels: Some(labels.clone()),
                    owner_references: self.owner_ref.clone(),
                    ..Default::default()
                }),
                spec: Some(api::ServiceSpec {
                    selector: Some(labels),
                    ports: Some(vec![port.to_service_port()]),
                    ..Default::default()
                }),
                ..Default::default()
            })
        })
    }
}

impl KubeName for ReplicatedService {
    fn kube_name(&self) -> String {
        self.instance_name.to_string()
    }
}

impl WorkloadType for ReplicatedService {
    fn add(&self) -> InstigatorResult {
        let deployment = self.to_deployment();
        let deployments =
            kube::api::Api::v1Deployment(self.client.clone()).within(self.namespace.as_str());
        let pp = PostParams::default();
        deployments.create(&pp, serde_json::to_vec(&deployment)?)?;

        match self.to_service() {
            Some(svc) => {
                info!("Service:\n{}", to_json(&svc).unwrap());
                let pp = PostParams::default();
                kube::api::Api::v1Service(self.client.clone())
                    .within(self.namespace.as_str())
                    .create(&pp, serde_json::to_vec(&svc)?)?;
                Ok(())
            }
            // No service to create
            None => {
                info!("Not attaching service to pod with no container ports.");
                Ok(())
            }
        }
    }
}
