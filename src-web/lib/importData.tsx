import type { BatchUpsertResult, Folder, HttpRequest } from '@yaakapp-internal/models';
import { environmentsAtom, foldersAtom, httpRequestsAtom } from '@yaakapp-internal/models';
import type { ImportResources } from '@yaakapp-internal/plugins';
import { Button } from '../components/core/Button';
import { FormattedError } from '../components/core/FormattedError';
import { VStack } from '../components/core/Stacks';
import { ImportDataDialog } from '../components/ImportDataDialog';
import { activeWorkspaceAtom, activeWorkspaceIdAtom } from '../hooks/useActiveWorkspace';
import { createFastMutation } from '../hooks/useFastMutation';
import { showAlert } from './alert';
import { showDialog } from './dialog';
import { jotaiStore } from './jotai';
import { pluralizeCount } from './pluralize';
import { router } from './router';
import { invokeCmd } from './tauri';

export const importData = createFastMutation({
  mutationKey: ['import_data'],
  onError: (err: string) => {
    showAlert({
      id: 'import-failed',
      title: 'Import Failed',
      size: 'md',
      body: <FormattedError>{err}</FormattedError>,
    });
  },
  mutationFn: async () => {
    return new Promise<void>((resolve, reject) => {
      showDialog({
        id: 'import',
        title: 'Import Data',
        size: 'sm',
        render: ({ hide }) => {
          const importAndHide = async (filePath: string) => {
            try {
              const didImport = await performImport(filePath);
              if (!didImport) {
                return;
              }
              resolve();
            } catch (err) {
              reject(err);
            } finally {
              hide();
            }
          };
          return <ImportDataDialog importData={importAndHide} />;
        },
      });
    });
  },
});

async function performImport(filePath: string): Promise<boolean> {
  const activeWorkspace = jotaiStore.get(activeWorkspaceAtom);
  const resources = await invokeCmd<ImportResources>('cmd_get_import_result', {
    workspaceId: activeWorkspace?.id,
    filePath,
  });

  const imported = await new Promise<BatchUpsertResult>((resolve) => {
    showDialog({
      id: 'import-location',
      title: 'Import Resources',
      render: ({ hide }) => {
        return (
          <div>
            <Button
              onClick={async () => {
                const workspaceId = jotaiStore.get(activeWorkspaceIdAtom);
                if (workspaceId == null) return;

                const newResources = structuredClone(resources);
                newResources.workspaces = [];

                console.log('IMPORTING', JSON.parse(JSON.stringify(resources, null, 1)));
                const idMap: Record<string, string> = {};

                const getParentNames = (m: HttpRequest | Folder): string[] => {
                  const parent = [...folders, ...resources.folders].find(
                    (f) => (idMap[f.id] ?? f.id) === (idMap[m.folderId ?? 'n/a'] ?? m.folderId),
                  );
                  if (parent == null) return [];
                  return [parent.name, ...getParentNames(parent)];
                };

                const folders = jotaiStore.get(foldersAtom);
                for (const toImport of newResources.folders) {
                  const parentsKey = getParentNames(toImport).join('::');
                  const existing = folders.find(
                    (m) => m.name === toImport.name && getParentNames(m).join('::') === parentsKey,
                  );
                  toImport.workspaceId = workspaceId;
                  if (existing) {
                    idMap[toImport.id] = existing.id;

                    toImport.id = existing.id;
                    toImport.folderId = existing.folderId;
                  }
                }

                const environments = jotaiStore.get(environmentsAtom);
                for (const toImport of newResources.environments) {
                  const existing = environments.find((m) => m.name === toImport.name);
                  toImport.workspaceId = workspaceId;
                  if (existing) {
                    toImport.id = existing.id;
                  }
                }

                const httpRequests = jotaiStore.get(httpRequestsAtom);
                for (const toImport of newResources.httpRequests) {
                  const parentsKey = getParentNames(toImport).join('::');
                  console.log('IMPORTING REQUEST', parentsKey, toImport.name);
                  const existing = httpRequests.find((m) => {
                    return (
                      m.name === toImport.name &&
                      m.url === toImport.url &&
                      m.method === toImport.method &&
                      getParentNames(m).join('::') === parentsKey
                    );
                  });
                  toImport.workspaceId = workspaceId;
                  if (existing) {
                    console.log('EXISTING');
                    toImport.id = existing.id;
                    toImport.folderId = existing.folderId;
                  } else {
                    console.log('NEW', toImport);
                  }
                }

                console.log('UPSERTING', newResources);
                const imported = await invokeCmd<BatchUpsertResult>('cmd_import_data', {
                  workspaceId: activeWorkspace?.id,
                  resources: newResources,
                });
                resolve(imported);
                hide();
              }}
            >
              Import into current
            </Button>
            <Button
              onClick={async () => {
                const imported = await invokeCmd<BatchUpsertResult>('cmd_import_data', {
                  workspaceId: activeWorkspace?.id,
                  resources,
                });
                resolve(imported);
                hide();
              }}
            >
              Import into new
            </Button>
          </div>
        );
      },
    });
  });

  console.log('IMPORTED', imported);
  const importedWorkspace = imported.workspaces[0];

  showDialog({
    id: 'import-complete',
    title: 'Import Complete',
    size: 'sm',
    hideX: true,
    render: ({ hide }) => {
      return (
        <VStack space={3} className="pb-4">
          <ul className="list-disc pl-6">
            {imported.workspaces.length > 0 && (
              <li>{pluralizeCount('Workspace', imported.workspaces.length)}</li>
            )}
            {imported.environments.length > 0 && (
              <li>{pluralizeCount('Environment', imported.environments.length)}</li>
            )}
            {imported.folders.length > 0 && (
              <li>{pluralizeCount('Folder', imported.folders.length)}</li>
            )}
            {imported.httpRequests.length > 0 && (
              <li>{pluralizeCount('HTTP Request', imported.httpRequests.length)}</li>
            )}
            {imported.grpcRequests.length > 0 && (
              <li>{pluralizeCount('GRPC Request', imported.grpcRequests.length)}</li>
            )}
            {imported.websocketRequests.length > 0 && (
              <li>{pluralizeCount('Websocket Request', imported.websocketRequests.length)}</li>
            )}
          </ul>
          <div>
            <Button className="ml-auto" onClick={hide} color="primary">
              Done
            </Button>
          </div>
        </VStack>
      );
    },
  });

  if (importedWorkspace != null) {
    const environmentId = imported.environments[0]?.id ?? null;
    await router.navigate({
      to: '/workspaces/$workspaceId',
      params: { workspaceId: importedWorkspace.id },
      search: { environment_id: environmentId },
    });
  }

  return true;
}
