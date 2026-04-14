"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VibeKanbanMqtt = void 0;
class VibeKanbanMqtt {
    name = 'vibeKanbanMqtt';
    displayName = 'Vibe Kanban MQTT';
    documentationUrl = 'https://github.com/noop';
    properties = [
        {
            displayName: 'Broker URL',
            name: 'brokerUrl',
            type: 'string',
            default: 'mqtt://127.0.0.1:1883',
            placeholder: 'mqtt://127.0.0.1:1883',
            required: true,
            description: 'MQTT broker used by the VK orchestration event publisher',
        },
        {
            displayName: 'Topic Namespace',
            name: 'topicNamespace',
            type: 'string',
            default: 'vk/orchestration',
            required: true,
            description: 'Namespace prefix configured by VK for orchestration topics',
        },
        {
            displayName: 'Username',
            name: 'username',
            type: 'string',
            default: '',
        },
        {
            displayName: 'Password',
            name: 'password',
            type: 'string',
            typeOptions: { password: true },
            default: '',
        },
        {
            displayName: 'Client ID',
            name: 'clientId',
            type: 'string',
            default: '',
            description: 'Optional MQTT client identifier. Leave blank to auto-generate one.',
        },
        {
            displayName: 'Clean Session',
            name: 'clean',
            type: 'boolean',
            default: true,
        }
    ];
}
exports.VibeKanbanMqtt = VibeKanbanMqtt;
//# sourceMappingURL=VibeKanbanMqtt.credentials.js.map