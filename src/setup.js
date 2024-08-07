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
      window: {
        id: id,
        body: args,
      },
    })
  );

  // Caller waits
  return out;
};

window._wongus.external_ipc = (id, args) => {
  try {
    const value = window.wongus.handle_external_ipc(args);
    window.ipc.postMessage(
      JSON.stringify({
        external: {
          id: id,
          body: {
            ok: value,
          },
        },
      })
    );
  } catch (e) {
    window.ipc.postMessage(
      JSON.stringify({
        external: {
          id: id,
          body: {
            err: e.toString(),
          },
        },
      })
    );
  }
};

window.wongus = {
  env: new Map(),
  args: new Map(),
  log: async (message) => {
    var message1 = message;
    if (!(message instanceof String)) {
      message = JSON.stringify(message);
    }
    return await wongus_ipc({ log: message });
  },
  read: async (path) => {
    return await wongus_ipc({ read: path });
  },
  run_command: async (args) => {
    return await wongus_ipc({ run_command: args });
  },
  run_independent: async (args) => {
    return await wongus_ipc({ run_independent: args });
  },
  stream_command: async (args) => {
    const cb = args.cb;
    delete args.db;
    const cb_id = next_stream_command_id++;
    args.id = cb_id;
    window._wongus.stream_cbs.set(cb_id, cb);
    return await wongus_ipc({ stream_command: args });
  },
  handle_external_ipc: null,
};
