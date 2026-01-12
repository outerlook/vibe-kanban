export const paths = {
  projects: () => '/projects',
  projectTasks: (projectId: string) => `/projects/${projectId}/tasks`,
  projectGantt: (projectId: string) => `/projects/${projectId}/gantt`,
  ganttBase: (projectId: string) => `/projects/${projectId}/gantt`,
  ganttTask: (projectId: string, taskId: string) =>
    `/projects/${projectId}/gantt/tasks/${taskId}`,
  ganttAttempt: (projectId: string, taskId: string, attemptId: string) =>
    `/projects/${projectId}/gantt/tasks/${taskId}/attempts/${attemptId}`,
  task: (projectId: string, taskId: string) =>
    `/projects/${projectId}/tasks/${taskId}`,
  attempt: (projectId: string, taskId: string, attemptId: string) =>
    `/projects/${projectId}/tasks/${taskId}/attempts/${attemptId}`,
  attemptFull: (projectId: string, taskId: string, attemptId: string) =>
    `/projects/${projectId}/tasks/${taskId}/attempts/${attemptId}/full`,
};
