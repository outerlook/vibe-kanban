"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.parseExecutorConfigs = parseExecutorConfigs;
exports.buildExecutorProfileId = buildExecutorProfileId;
exports.toExecutorOptions = toExecutorOptions;
exports.toExecutorVariantOptions = toExecutorVariantOptions;
function toDisplayLabel(value) {
    return value
        .split('_')
        .map((segment) => segment.length === 0
        ? segment
        : `${segment[0].toUpperCase()}${segment.slice(1).toLowerCase()}`)
        .join(' ');
}
function parseExecutorConfigs(raw) {
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || !parsed.executors) {
        throw new Error('VK profiles payload is missing the executors catalog');
    }
    return parsed;
}
function buildExecutorProfileId(executor, variant) {
    if (!executor) {
        return null;
    }
    return {
        executor: executor,
        variant: variant || null,
    };
}
function toExecutorOptions(profiles) {
    return Object.keys(profiles.executors)
        .sort()
        .map((executor) => ({
        name: toDisplayLabel(executor),
        value: executor,
    }));
}
function toExecutorVariantOptions(profiles, executor) {
    const options = [
        {
            name: 'Default',
            value: '',
        },
    ];
    if (!executor) {
        return options;
    }
    const config = profiles.executors[executor];
    if (!config) {
        return options;
    }
    const variants = Object.keys(config)
        .filter((variant) => variant !== 'DEFAULT')
        .sort();
    return options.concat(variants.map((variant) => ({
        name: toDisplayLabel(variant),
        value: variant,
    })));
}
//# sourceMappingURL=executor-profiles.js.map