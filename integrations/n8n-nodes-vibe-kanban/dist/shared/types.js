"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.DEFAULT_COMMIT_MESSAGE_PROMPT = exports.DEFAULT_PR_DESCRIPTION_PROMPT = exports.BaseAgentCapability = exports.BaseCodingAgent = exports.SoundFile = exports.EditorType = exports.ThemeMode = exports.InvitationStatus = exports.MemberRole = exports.ExecutionProcessStatus = exports.ScratchType = void 0;
var ScratchType;
(function (ScratchType) {
    ScratchType["DRAFT_TASK"] = "DRAFT_TASK";
    ScratchType["DRAFT_FOLLOW_UP"] = "DRAFT_FOLLOW_UP";
    ScratchType["DRAFT_CONVERSATION_MESSAGE"] = "DRAFT_CONVERSATION_MESSAGE";
})(ScratchType || (exports.ScratchType = ScratchType = {}));
var ExecutionProcessStatus;
(function (ExecutionProcessStatus) {
    ExecutionProcessStatus["running"] = "running";
    ExecutionProcessStatus["completed"] = "completed";
    ExecutionProcessStatus["failed"] = "failed";
    ExecutionProcessStatus["killed"] = "killed";
})(ExecutionProcessStatus || (exports.ExecutionProcessStatus = ExecutionProcessStatus = {}));
var MemberRole;
(function (MemberRole) {
    MemberRole["ADMIN"] = "ADMIN";
    MemberRole["MEMBER"] = "MEMBER";
})(MemberRole || (exports.MemberRole = MemberRole = {}));
var InvitationStatus;
(function (InvitationStatus) {
    InvitationStatus["PENDING"] = "PENDING";
    InvitationStatus["ACCEPTED"] = "ACCEPTED";
    InvitationStatus["DECLINED"] = "DECLINED";
    InvitationStatus["EXPIRED"] = "EXPIRED";
})(InvitationStatus || (exports.InvitationStatus = InvitationStatus = {}));
var ThemeMode;
(function (ThemeMode) {
    ThemeMode["LIGHT"] = "LIGHT";
    ThemeMode["DARK"] = "DARK";
    ThemeMode["SYSTEM"] = "SYSTEM";
})(ThemeMode || (exports.ThemeMode = ThemeMode = {}));
var EditorType;
(function (EditorType) {
    EditorType["VS_CODE"] = "VS_CODE";
    EditorType["CURSOR"] = "CURSOR";
    EditorType["WINDSURF"] = "WINDSURF";
    EditorType["INTELLI_J"] = "INTELLI_J";
    EditorType["ZED"] = "ZED";
    EditorType["XCODE"] = "XCODE";
    EditorType["CUSTOM"] = "CUSTOM";
})(EditorType || (exports.EditorType = EditorType = {}));
var SoundFile;
(function (SoundFile) {
    SoundFile["ABSTRACT_SOUND1"] = "ABSTRACT_SOUND1";
    SoundFile["ABSTRACT_SOUND2"] = "ABSTRACT_SOUND2";
    SoundFile["ABSTRACT_SOUND3"] = "ABSTRACT_SOUND3";
    SoundFile["ABSTRACT_SOUND4"] = "ABSTRACT_SOUND4";
    SoundFile["COW_MOOING"] = "COW_MOOING";
    SoundFile["ERROR_BUZZER"] = "ERROR_BUZZER";
    SoundFile["PHONE_VIBRATION"] = "PHONE_VIBRATION";
    SoundFile["ROOSTER"] = "ROOSTER";
})(SoundFile || (exports.SoundFile = SoundFile = {}));
var BaseCodingAgent;
(function (BaseCodingAgent) {
    BaseCodingAgent["CLAUDE_CODE"] = "CLAUDE_CODE";
    BaseCodingAgent["AMP"] = "AMP";
    BaseCodingAgent["GEMINI"] = "GEMINI";
    BaseCodingAgent["CODEX"] = "CODEX";
    BaseCodingAgent["OPENCODE"] = "OPENCODE";
    BaseCodingAgent["CURSOR_AGENT"] = "CURSOR_AGENT";
    BaseCodingAgent["QWEN_CODE"] = "QWEN_CODE";
    BaseCodingAgent["COPILOT"] = "COPILOT";
    BaseCodingAgent["DROID"] = "DROID";
})(BaseCodingAgent || (exports.BaseCodingAgent = BaseCodingAgent = {}));
var BaseAgentCapability;
(function (BaseAgentCapability) {
    BaseAgentCapability["SESSION_FORK"] = "SESSION_FORK";
    BaseAgentCapability["SETUP_HELPER"] = "SETUP_HELPER";
})(BaseAgentCapability || (exports.BaseAgentCapability = BaseAgentCapability = {}));
exports.DEFAULT_PR_DESCRIPTION_PROMPT = `Update the GitHub PR that was just created with a better title and description.
The PR number is #{pr_number} and the URL is {pr_url}.

Analyze the changes in this branch and write:
1. A concise, descriptive title that summarizes the changes, postfixed with "(Vibe Kanban)"
2. A detailed description that explains:
   - What changes were made
   - Why they were made (based on the task context)
   - Any important implementation details
   - At the end, include a note: "This PR was written using [Vibe Kanban](https://vibekanban.com)"

Use \`gh pr edit\` to update the PR.`;
exports.DEFAULT_COMMIT_MESSAGE_PROMPT = `Generate a concise git commit message for the following changes.

Task: {task_title}
Description: {task_description}

Diff:
{diff}

Write a commit message following these guidelines:
- First line: imperative mood summary (50 chars max)
- Blank line
- Body: explain what and why (wrap at 72 chars)

Respond with ONLY the commit message, no other text.`;
//# sourceMappingURL=types.js.map