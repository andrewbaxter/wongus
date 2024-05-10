window._wongus = {
  stream_cbs: new Map(),
  responses: new Map(),
};
var next_ipc_id = 0;
var next_stream_command_id = 0;

const wongus_ipc = (args) => {
  // Client id
  const id = next_ipc_id++;

  // Prep response handler, promise
  const out = new Promise((resolve, reject) => {
    window._wongus.responses.set(id, (resp) => {
      try {
        window._wongus.responses.delete(id);
        if (resp.err) {
          reject(new Error(resp.err));
          return;
        }
        resolve(resp);
      } catch (e) {
        reject(e);
        return;
      }
    });
  });

  // Send req
  window.ipc.postMessage(
    JSON.stringify({
      id: id,
      body: args,
    })
  );

  // Caller waits
  return out;
};

window.wongus = {
  env: new Map(),
  args: new Map(),
  read: async (path) => {
    return await wongus_ipc({ read: path });
  },
  run_command: async (args) => {
    return await wongus_ipc({ run_command: args });
  },
  stream_command: async (args) => {
    const cb = args.cb;
    delete args.db;
    const cb_id = next_stream_command_id++;
    args.id = cb_id;
    window._wongus.stream_cbs.set(cb_id, cb);
    return await wongus_ipc({ stream_command: args });
  },
};
const old_log = window.console.log;
window.console.log = (...args) => {
  old_log(...args);
  wongus_ipc({ log: args.map((a) => String(a)).join(" ") });
};