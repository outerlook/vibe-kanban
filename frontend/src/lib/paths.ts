export const paths = {
  projects: () => '/projects',
  projectTasks: (projectId: string) => `/projects/${projectId}/tasks`,
  projectConversations: (projectId: string) =>
    `/projects/${projectId}/conversations`,
  conversation: (projectId: string, conversationId: string) =>
    `/projects/${projectId}/conversations/${conversationId}`,
  projectGantt: (projectId: string) => `/projects/${projectId}/gantt`,
  projectPrs: (projectId: string) => `/projects/${projectId}/prs`,
  ganttTask: (projectId: string, taskId: string) =>
    `/projects/${projectId}/gantt/${taskId}`,
  ganttAttempt: (projectId: string, taskId: string, attemptId: string) =>
    `/projects/${projectId}/gantt/${taskId}/attempts/${attemptId}`,
  task: (projectId: string, taskId: string) =>
    `/projects/${projectId}/tasks/${taskId}`,
  attempt: (projectId: string, taskId: string, attemptId: string) =>
    `/projects/${projectId}/tasks/${taskId}/attempts/${attemptId}`,
  attemptFull: (projectId: string, taskId: string, attemptId: string) =>
    `/projects/${projectId}/tasks/${taskId}/attempts/${attemptId}/full`,
};
