import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const examplesDir = join(import.meta.dirname, "..", "examples");

describe("production workflows", () => {
  it("ships first-party workflows for the supported VK and GitHub automations", () => {
    const exampleFiles = readdirSync(examplesDir)
      .filter((file) => file.endsWith(".workflow.json"))
      .sort();

    expect(exampleFiles).toEqual(
      expect.arrayContaining([
        "autopilot-continuation.workflow.json",
        "coderabbit-review-extraction.workflow.json",
        "feedback-collection.workflow.json",
        "generate-and-merge-follow-up.workflow.json",
        "review-attention.workflow.json",
      ]),
    );
  });

  it("keeps VK-backed workflows on the VK n8n contract", () => {
    for (const file of readdirSync(examplesDir).filter((entry) =>
      entry.endsWith(".workflow.json"),
    )) {
      const workflow = JSON.parse(
        readFileSync(join(examplesDir, file), "utf8"),
      ) as {
        nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
      };

      expect(workflow.nodes.length).toBeGreaterThan(0);

      const vkNodes = workflow.nodes.filter((node) =>
        node.type.startsWith("CUSTOM."),
      );
      if (vkNodes.length === 0) {
        continue;
      }

      for (const node of vkNodes) {
        if (node.type === "CUSTOM.vibeKanbanTrigger") {
          expect(node.parameters?.schemaVersion).toBe(
            "vk_n8n_orchestration_v1",
          );
        }
      }
    }
  });

  it("marks every shipped workflow as production-ready in the canvas note", () => {
    for (const file of readdirSync(examplesDir).filter((entry) =>
      entry.endsWith(".workflow.json"),
    )) {
      const workflow = JSON.parse(
        readFileSync(join(examplesDir, file), "utf8"),
      ) as {
        nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
      };

      const descriptionNode = workflow.nodes.find(
        (node) => node.type === "n8n-nodes-base.stickyNote",
      );

      expect(descriptionNode?.parameters?.content).toContain(
        "Production Workflow",
      );
      expect(descriptionNode?.parameters?.content).not.toContain("Use this as");
    }
  });

  it("starts the review gate when tasks enter in_review and launches a VK reviewer conversation", () => {
    const workflow = JSON.parse(
      readFileSync(join(examplesDir, "review-attention.workflow.json"), "utf8"),
    ) as {
      nodes: Array<{
        name?: string;
        type: string;
        parameters?: Record<string, unknown>;
      }>;
    };

    const serialized = JSON.stringify(workflow);
    const promptNode = workflow.nodes.find(
      (node) => node.name === "Build Review Conversation Input",
    );
    const promptAssignments = promptNode?.parameters?.assignments as
      | {
          assignments?: Array<{
            name?: string;
            value?: string;
          }>;
        }
      | undefined;
    const initialMessage = promptAssignments?.assignments?.find(
      (assignment) => assignment.name === "initialMessage",
    )?.value;

    expect(serialized).toContain("payload?.status || '') === 'in_review'");
    expect(serialized).toContain("latest_coding_execution");
    expect(serialized).toContain('"operation":"createConversation"');
    expect(serialized).toContain('"executor":""');
    expect(serialized).toContain('"executorVariant":""');
    expect(serialized).toContain("reviewGate.pendingByReviewExecutionProcessId");
    expect(serialized).toContain("agent_working_dir");
    expect(initialMessage).toContain("## Agent's Work Summary");
    expect(initialMessage).toContain("\"needs_attention\": <true if problems OR task objective not addressed>");
    expect(serialized).not.toContain("@n8n/n8n-nodes-langchain");
  });

  it("starts autopilot only when tasks enter done and filters dependents in workflow code", () => {
    const workflow = JSON.parse(
      readFileSync(
        join(examplesDir, "autopilot-continuation.workflow.json"),
        "utf8",
      ),
    ) as {
      nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
    };

    const serialized = JSON.stringify(workflow);
    expect(
      workflow.nodes.some((node) => node.type === "n8n-nodes-base.code"),
    ).toBe(true);
    expect(serialized).toContain("payload?.status || '') === 'done'");
    expect(serialized).toContain("dependency_context?.dependents");
    expect(serialized).toContain("task.status === 'todo'");
    expect(serialized).toContain('operation":"startTaskExecution');
    expect(serialized).toContain("latest_or_create");
    expect(serialized).toContain('"executorStrategy":"default"');
    expect(serialized).not.toContain("latest_or_default");
  });

  it("keeps feedback and stored review results task-scoped", () => {
    const feedbackWorkflow = JSON.parse(
      readFileSync(join(examplesDir, "feedback-collection.workflow.json"), "utf8"),
    ) as {
      nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
    };

    const feedbackSerialized = JSON.stringify(feedbackWorkflow);
    expect(feedbackSerialized).toContain("scope?.task?.id");
    expect(feedbackSerialized).toContain("scope?.workspace?.id");

    const reviewWorkflow = JSON.parse(
      readFileSync(
        join(examplesDir, "generate-and-merge-follow-up.workflow.json"),
        "utf8",
      ),
    ) as {
      nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
    };

    const reviewSerialized = JSON.stringify(reviewWorkflow);
    expect(reviewSerialized).toContain('"resource":"reviewAttention"');
    expect(reviewSerialized).toContain('"operation":"createReviewAttention"');
    expect(reviewSerialized).toContain('"taskId":"={{$json.taskId}}"');
    expect(reviewSerialized).toContain('"workspaceId":"={{$json.workspaceId}}"');
  });

  it("applies reviewer verdicts after reviewer execution completes and only queues merge when approved", () => {
    const workflow = JSON.parse(
      readFileSync(
        join(examplesDir, "generate-and-merge-follow-up.workflow.json"),
        "utf8",
      ),
    ) as {
      nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
    };

    const serialized = JSON.stringify(workflow);
    expect(serialized).toContain('"eventTypes":["execution_completed"]');
    expect(serialized).toContain("pendingByReviewExecutionProcessId");
    expect(serialized).toContain('"resource":"conversation"');
    expect(serialized).toContain("Failed to parse reviewer verdict");
    expect(serialized).toContain("needs_attention as a boolean");
    expect(serialized).toContain('"operation":"createReviewAttention"');
    expect(serialized).toContain('"operation":"generateCommitMessage"');
    expect(serialized).toContain('"operation":"queueMerge"');
    expect(serialized).toContain('"executor":""');
    expect(serialized).not.toContain("approval_resolved");
  });

  it("polls open PRs and keeps only unresolved CodeRabbit review threads", () => {
    const workflow = JSON.parse(
      readFileSync(
        join(examplesDir, "coderabbit-review-extraction.workflow.json"),
        "utf8",
      ),
    ) as {
      nodes: Array<{ type: string; parameters?: Record<string, unknown> }>;
    };

    expect(
      workflow.nodes.some(
        (node) => node.type === "n8n-nodes-base.scheduleTrigger",
      ),
    ).toBe(true);

    expect(
      workflow.nodes.some((node) => node.type === "n8n-nodes-base.code"),
    ).toBe(true);

    const serialized = JSON.stringify(workflow);
    expect(serialized).toContain("/graphql");
    expect(serialized).toContain('"nodeCredentialType":"githubApi"');
    expect(serialized).toContain('"authentication":"predefinedCredentialType"');
    expect(serialized).toContain('"resource":"githubRepositories"');
    expect(serialized).toContain('"type":"CUSTOM.vibeKanbanSelect"');
    expect(serialized).toContain('"workflowId":"={{$json.workflowId}}"');
    expect(serialized).toContain("reviewThreads(first: 100");
    expect(serialized).toContain("isResolved");
    expect(serialized).toContain("pullRequests(states: OPEN");
    expect(serialized).toContain("coderabbitai[bot]");
    expect(serialized).toContain("$getWorkflowStaticData");
    expect(serialized).toContain("processedThreadCommentIds");
    expect(serialized).not.toContain("GITHUB_TOKEN");
    expect(serialized).not.toContain("allowedReposJson");
    expect(serialized).not.toContain("ignoredReposJson");
    expect(serialized).not.toContain("allowedProjectIdsJson");
    expect(serialized).not.toContain("vkBaseUrl");
  });
});
