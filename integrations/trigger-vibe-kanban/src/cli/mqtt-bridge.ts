import { createRuntimeDependencies } from '../runtime/dependencies';
import { startMqttBridgeRuntime } from '../runtime/mqtt-bridge';

const dependencies = createRuntimeDependencies();
const runtime = await startMqttBridgeRuntime({ dependencies });

for (const signal of ['SIGINT', 'SIGTERM'] as const) {
  process.once(signal, async () => {
    await runtime.close();
    dependencies.stateStore.close();
    process.exit(0);
  });
}
