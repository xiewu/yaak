use crate::error::Result;
use log::info;
use std::collections::BTreeMap;
use std::fs::read_to_string;
use tauri::{Manager, Runtime, WebviewWindow};
use yaak_models::models::{
    AnyModel, Environment, Folder, GrpcRequest, HttpRequest, WebsocketRequest, Workspace,
};
use yaak_models::query_manager::QueryManagerExt;
use yaak_models::util::{
    BatchUpsertResult, UpdateSource, get_workspace_export_resources, maybe_gen_id, maybe_gen_id_opt,
};
use yaak_plugins::events::ImportResources;
use yaak_plugins::manager::PluginManager;

pub(crate) async fn get_import_result<R: Runtime>(
    window: &WebviewWindow<R>,
    file_path: &str,
) -> Result<ImportResources> {
    let plugin_manager = window.state::<PluginManager>();
    let file =
        read_to_string(file_path).unwrap_or_else(|_| panic!("Unable to read file {}", file_path));
    let file_contents = file.as_str();
    let import_result = plugin_manager.import_data(window, file_contents).await?;
    Ok(import_result.resources)
}

pub(crate) fn import_data_merge<R: Runtime>(
    window: &WebviewWindow<R>,
    importing_resources: &ImportResources,
    into_workspace_id: &str,
) -> Result<ImportResources> {
    let existing =
        get_workspace_export_resources(window.app_handle(), vec![into_workspace_id], true)?
            .resources;
    let existing_resources = ImportResources {
        workspaces: existing.workspaces,
        environments: existing.environments,
        folders: existing.folders,
        http_requests: existing.http_requests,
        grpc_requests: existing.grpc_requests,
        websocket_requests: existing.websocket_requests,
    };

    Ok(merge_resources(importing_resources, &existing_resources))
}

pub(crate) fn merge_resources(
    import_resources: &ImportResources,
    existing_resources: &ImportResources,
) -> ImportResources {
    let import_key_map: BTreeMap<String, AnyModel> = compute_key_map(import_resources);
    let existing_key_map: BTreeMap<String, AnyModel> = compute_key_map(&existing_resources);

    let resources_to_upsert = ImportResources {
        ..Default::default()
    };

    for

    resources_to_upsert
}

fn compute_key_map(resources: &ImportResources) -> BTreeMap<String, AnyModel> {
    let mut key_map = BTreeMap::<String, AnyModel>::new();
    for m in resources.folders.clone() {
        let am: AnyModel = m.into();
        let key = compute_key(&am, resources);
        key_map.insert(key, am);
    }
    for m in resources.http_requests.clone() {
        let am: AnyModel = m.into();
        let key = compute_key(&am, resources);
        key_map.insert(key, am);
    }
    key_map
}

fn compute_key(model: &AnyModel, resources: &ImportResources) -> String {
    match model.clone() {
        AnyModel::Folder(m) => {
            compute_folder_key(Some(m.id), &resources.folders).unwrap_or_default()
        }
        AnyModel::HttpRequest(m) => match compute_folder_key(m.folder_id, &resources.folders) {
            None => m.name,
            Some(k) => format!("{k}::{}", m.name),
        },
        m => m.id().to_string(),
    }
}

fn compute_folder_key(id: Option<String>, folders: &Vec<Folder>) -> Option<String> {
    let id = match id {
        None => return None,
        Some(v) => v,
    };

    let folder = match folders.iter().find(|f| f.id == id) {
        None => return None,
        Some(v) => v,
    };

    let parent = match folder.folder_id.clone() {
        None => None,
        Some(fid) => folders.iter().find(|f| f.id == fid),
    };
    let parent_chain = match parent {
        Some(p) => compute_folder_key(Some(p.id.clone()), folders),
        None => None,
    };

    let chain = match parent_chain {
        None => folder.name.clone(),
        Some(c) => format!("{}::{}", c, folder.name),
    };

    Some(chain)
}

pub(crate) fn import_data<R: Runtime>(
    window: &WebviewWindow<R>,
    resources: ImportResources,
) -> Result<BatchUpsertResult> {
    let mut id_map: BTreeMap<String, String> = BTreeMap::new();

    let workspaces: Vec<Workspace> = resources
        .workspaces
        .into_iter()
        .map(|mut v| {
            v.id = maybe_gen_id::<Workspace, R>(window, v.id.as_str(), &mut id_map);
            v
        })
        .collect();

    let environments: Vec<Environment> = resources
        .environments
        .into_iter()
        .map(|mut v| {
            v.id = maybe_gen_id::<Environment, R>(window, v.id.as_str(), &mut id_map);
            v.workspace_id =
                maybe_gen_id::<Workspace, R>(window, v.workspace_id.as_str(), &mut id_map);
            match (v.parent_model.as_str(), v.parent_id.clone().as_deref()) {
                ("folder", Some(parent_id)) => {
                    v.parent_id = Some(maybe_gen_id::<Folder, R>(window, &parent_id, &mut id_map));
                }
                ("", _) => {
                    // Fix any empty ones
                    v.parent_model = "workspace".to_string();
                }
                _ => {
                    // Parent ID only required for the folder case
                    v.parent_id = None;
                }
            };
            v
        })
        .collect();

    let folders: Vec<Folder> = resources
        .folders
        .into_iter()
        .map(|mut v| {
            v.id = maybe_gen_id::<Folder, R>(window, v.id.as_str(), &mut id_map);
            v.workspace_id =
                maybe_gen_id::<Workspace, R>(window, v.workspace_id.as_str(), &mut id_map);
            v.folder_id = maybe_gen_id_opt::<Folder, R>(window, v.folder_id, &mut id_map);
            v
        })
        .collect();

    let http_requests: Vec<HttpRequest> = resources
        .http_requests
        .into_iter()
        .map(|mut v| {
            v.id = maybe_gen_id::<HttpRequest, R>(window, v.id.as_str(), &mut id_map);
            v.workspace_id =
                maybe_gen_id::<Workspace, R>(window, v.workspace_id.as_str(), &mut id_map);
            v.folder_id = maybe_gen_id_opt::<Folder, R>(window, v.folder_id, &mut id_map);
            v
        })
        .collect();

    let grpc_requests: Vec<GrpcRequest> = resources
        .grpc_requests
        .into_iter()
        .map(|mut v| {
            v.id = maybe_gen_id::<GrpcRequest, R>(window, v.id.as_str(), &mut id_map);
            v.workspace_id =
                maybe_gen_id::<Workspace, R>(window, v.workspace_id.as_str(), &mut id_map);
            v.folder_id = maybe_gen_id_opt::<Folder, R>(window, v.folder_id, &mut id_map);
            v
        })
        .collect();

    let websocket_requests: Vec<WebsocketRequest> = resources
        .websocket_requests
        .into_iter()
        .map(|mut v| {
            v.id = maybe_gen_id::<WebsocketRequest, R>(window, v.id.as_str(), &mut id_map);
            v.workspace_id =
                maybe_gen_id::<Workspace, R>(window, v.workspace_id.as_str(), &mut id_map);
            v.folder_id = maybe_gen_id_opt::<Folder, R>(window, v.folder_id, &mut id_map);
            v
        })
        .collect();

    info!("Importing data");

    let upserted = window.with_tx(|tx| {
        tx.batch_upsert(
            workspaces,
            environments,
            folders,
            http_requests,
            grpc_requests,
            websocket_requests,
            &UpdateSource::Import,
        )
    })?;

    Ok(upserted)
}

#[cfg(test)]
mod tests {
    use crate::import::{compute_key_map, import_data_merge, merge_resources};
    use yaak_models::models::{Folder, HttpRequest};
    use yaak_plugins::events::ImportResources;

    #[test]
    fn do_it() {
        let f_a = Folder {
            id: "f_a".to_string(),
            name: "Folder A".to_string(),
            ..Default::default()
        };

        let f_b = Folder {
            id: "f_b".to_string(),
            name: "Folder B".to_string(),
            folder_id: Some(f_a.id.clone()),
            ..Default::default()
        };

        let r_a = HttpRequest {
            id: "r_a".to_string(),
            name: "Request A".to_string(),
            ..Default::default()
        };

        let r_b = HttpRequest {
            id: "r_b".to_string(),
            name: "Request B".to_string(),
            folder_id: Some(f_b.id.clone()),
            ..Default::default()
        };

        let resources = ImportResources {
            folders: vec![f_a.clone(), f_b.clone()],
            http_requests: vec![r_a.clone(), r_b.clone()],
            ..Default::default()
        };

        let keys = compute_key_map(&resources);
        assert_eq!(
            keys.keys().collect::<Vec<_>>(),
            vec![
                "Folder A",
                "Folder A::Folder B",
                "Folder A::Folder B::Request B",
                "Request A"
            ]
        )
    }

    #[test]
    fn merge_resources_simple() {
        let import_resources = ImportResources {
            http_requests: vec![
                HttpRequest {
                    id: "r_a".to_string(),
                    name: "Request A".to_string(),
                    ..Default::default()
                },
                HttpRequest {
                    id: "r_b".to_string(),
                    name: "Request B".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let existing_resources = ImportResources {
            http_requests: vec![HttpRequest {
                id: "r_a".to_string(),
                name: "Request A".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };

        let result = merge_resources(&import_resources, &existing_resources);
        assert_eq!(result.http_requests.len(), 2)
    }
}
