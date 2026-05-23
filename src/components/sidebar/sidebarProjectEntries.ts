import type { CodexDetectedProject, Thread, Workspace } from "../../types";

export interface ProjectGroup {
  workspace: Workspace;
  threads: Thread[];
}

export interface UnifiedProjectGroup {
  key: string;
  path: string;
  name: string;
  workspace: Workspace | null;
  detectedCodexProject: CodexDetectedProject | null;
  conversations: SidebarConversation[];
  totalConversationCount: number;
  latestActivityAt: string;
  isPinnedProject: boolean;
  pinnedProjectAt: string | null;
  hasPinnedConversation: boolean;
  latestPinnedActivityAt: string | null;
}

export type SidebarConversation =
  | {
      kind: "local";
      key: string;
      updatedAt: string;
      localThread: Thread;
    }
  | {
      kind: "detected";
      key: string;
      updatedAt: string;
      detectedThread: CodexDetectedProject["threads"][number];
    };

export type SidebarPinnedProjects = Record<string, string>;

export function isThreadPinned(thread: Thread): boolean {
  return thread.engineMetadata?.pinned === true;
}

function pinnedConversationTimestamp(thread: Thread): number {
  const raw = thread.engineMetadata?.pinnedAt;
  if (typeof raw !== "string") {
    return 0;
  }

  const parsed = Date.parse(raw);
  return Number.isFinite(parsed) ? parsed : 0;
}

export function isProjectPinned(
  projectKey: string,
  pinnedProjects: SidebarPinnedProjects,
): boolean {
  return typeof pinnedProjects[projectKey] === "string";
}

function compareSidebarConversations(left: SidebarConversation, right: SidebarConversation): number {
  const leftPinned = left.kind === "local" && isThreadPinned(left.localThread);
  const rightPinned = right.kind === "local" && isThreadPinned(right.localThread);
  if (leftPinned !== rightPinned) {
    return leftPinned ? -1 : 1;
  }

  const activityDiff =
    new Date(right.updatedAt).getTime() - new Date(left.updatedAt).getTime();
  if (activityDiff !== 0) {
    return activityDiff;
  }

  if (leftPinned && rightPinned && left.kind === "local" && right.kind === "local") {
    const pinnedDiff =
      pinnedConversationTimestamp(right.localThread) - pinnedConversationTimestamp(left.localThread);
    if (pinnedDiff !== 0) {
      return pinnedDiff;
    }
  }

  return left.key.localeCompare(right.key);
}

export function buildSidebarProjectEntries(
  workspaceProjects: ProjectGroup[],
  detectedCodexProjects: CodexDetectedProject[],
  pinnedProjects: SidebarPinnedProjects,
): UnifiedProjectGroup[] {
  const detectedCodexProjectsByWorkspaceId = detectedCodexProjects.reduce<
    Record<string, CodexDetectedProject>
  >((acc, project) => {
    if (project.workspaceId) {
      acc[project.workspaceId] = project;
    }
    return acc;
  }, {});

  const importedProjects = workspaceProjects.map((project) => {
    const detectedCodexProject =
      detectedCodexProjectsByWorkspaceId[project.workspace.id] ?? null;
    const attachedEngineThreadIds = new Set(
      project.threads
        .map((thread) => thread.engineThreadId)
        .filter((engineThreadId): engineThreadId is string => Boolean(engineThreadId)),
    );
    const detectedConversations = (detectedCodexProject?.threads ?? [])
      .filter((thread) => !thread.archived && !attachedEngineThreadIds.has(thread.engineThreadId))
      .map<SidebarConversation>((thread) => ({
        kind: "detected",
        key: `detected:${thread.profileId}:${thread.engineThreadId}`,
        updatedAt: thread.updatedAt,
        detectedThread: thread,
      }));
    const localConversations = project.threads.map<Extract<SidebarConversation, { kind: "local" }>>((thread) => ({
      kind: "local",
      key: `local:${thread.id}`,
      updatedAt: thread.lastActivityAt,
      localThread: thread,
    }));
    const conversations = [...localConversations, ...detectedConversations].sort(
      compareSidebarConversations,
    );
    const pinnedLocalConversations = localConversations.filter((conversation) =>
      isThreadPinned(conversation.localThread),
    );
    const latestPinnedActivityAt =
      pinnedLocalConversations
        .sort(compareSidebarConversations)[0]
        ?.updatedAt ?? null;

    return {
      key: project.workspace.id,
      path: project.workspace.rootPath,
      name: project.workspace.name || project.workspace.rootPath.split("/").pop() || project.workspace.rootPath,
      workspace: project.workspace,
      detectedCodexProject,
      conversations,
      totalConversationCount: conversations.length,
      latestActivityAt: conversations[0]?.updatedAt ?? project.workspace.lastOpenedAt,
      isPinnedProject: isProjectPinned(project.workspace.id, pinnedProjects),
      pinnedProjectAt: pinnedProjects[project.workspace.id] ?? null,
      hasPinnedConversation: pinnedLocalConversations.length > 0,
      latestPinnedActivityAt,
    };
  });

  const externalProjects = detectedCodexProjects
    .filter((project) => !project.workspaceId)
    .map<UnifiedProjectGroup>((project) => {
      const conversations = project.threads
        .filter((thread) => !thread.archived)
        .map<SidebarConversation>((thread) => ({
          kind: "detected",
          key: `detected:${thread.profileId}:${thread.engineThreadId}`,
          updatedAt: thread.updatedAt,
          detectedThread: thread,
        }))
        .sort(compareSidebarConversations);

      return {
        key: `detected:${project.path}`,
        path: project.path,
        name: project.name,
        workspace: null,
        detectedCodexProject: project,
        conversations,
        totalConversationCount: conversations.length,
        latestActivityAt: conversations[0]?.updatedAt ?? project.lastActivityAt,
        isPinnedProject: isProjectPinned(
          `detected:${project.path}`,
          pinnedProjects,
        ),
        pinnedProjectAt:
          pinnedProjects[`detected:${project.path}`] ?? null,
        hasPinnedConversation: false,
        latestPinnedActivityAt: null,
      };
    })
    .filter((project) => project.conversations.length > 0);

  return [...importedProjects, ...externalProjects].sort((left, right) => {
    if (left.isPinnedProject !== right.isPinnedProject) {
      return left.isPinnedProject ? -1 : 1;
    }
    const projectPinTimeDiff =
      new Date(right.pinnedProjectAt ?? 0).getTime()
      - new Date(left.pinnedProjectAt ?? 0).getTime();
    if (projectPinTimeDiff !== 0) {
      return projectPinTimeDiff;
    }
    if (left.hasPinnedConversation !== right.hasPinnedConversation) {
      return left.hasPinnedConversation ? -1 : 1;
    }
    const pinnedTimeDiff =
      new Date(right.latestPinnedActivityAt ?? 0).getTime()
      - new Date(left.latestPinnedActivityAt ?? 0).getTime();
    if (pinnedTimeDiff !== 0) {
      return pinnedTimeDiff;
    }
    const timeDiff =
      new Date(right.latestActivityAt).getTime() - new Date(left.latestActivityAt).getTime();
    if (timeDiff !== 0) {
      return timeDiff;
    }
    return left.name.localeCompare(right.name, undefined, { sensitivity: "base" });
  });
}
