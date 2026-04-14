"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.VibeKanbanApi = void 0;
class VibeKanbanApi {
    name = 'vibeKanbanApi';
    displayName = 'Vibe Kanban API';
    documentationUrl = 'https://github.com/noop';
    properties = [
        {
            displayName: 'Base URL',
            name: 'baseUrl',
            type: 'string',
            default: 'http://127.0.0.1:3001',
            placeholder: 'https://vk.example.com',
            description: 'Base Vibe Kanban server URL without the /api suffix',
            required: true,
        },
        {
            displayName: 'Auth Mode',
            name: 'authMode',
            type: 'options',
            default: 'none',
            options: [
                { name: 'None', value: 'none' },
                { name: 'Bearer Token', value: 'bearerToken' },
                { name: 'Custom Header', value: 'customHeader' },
            ],
            description: 'How requests authenticate against the VK control plane',
        },
        {
            displayName: 'Token',
            name: 'token',
            type: 'string',
            typeOptions: { password: true },
            default: '',
            required: false,
            displayOptions: {
                show: {
                    authMode: ['bearerToken', 'customHeader'],
                },
            },
        },
        {
            displayName: 'Header Name',
            name: 'headerName',
            type: 'string',
            default: 'Authorization',
            required: true,
            displayOptions: {
                show: {
                    authMode: ['customHeader'],
                },
            },
        },
        {
            displayName: 'Header Prefix',
            name: 'headerPrefix',
            type: 'string',
            default: 'Bearer ',
            required: false,
            displayOptions: {
                show: {
                    authMode: ['bearerToken'],
                },
            },
            description: 'Prefix prepended before the token for bearer auth',
        },
    ];
}
exports.VibeKanbanApi = VibeKanbanApi;
//# sourceMappingURL=VibeKanbanApi.credentials.js.map