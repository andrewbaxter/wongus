window._wongus = {
  stream_cbs: new Map(),
  responses: new Map(),
};

const ipc = (args) => {
  // Client id
  const req = crypto.randomUUID();

  // Prep response handler, promise
  const out = new Promise((resolve, reject) => {
    window._wongus.responses.set(req, (resp) => {
      try {
        // Cleanup
        window._wongus.responses.delete(req);

        // Parse resp and respond
        resp = JSON.parse(resp);
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
      req: req,
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
    return await ipc({ read: path });
  },
  run_command: async (args) => {
    return await ipc({ run_command: args });
  },
  stream_command: async (args) => {
    const cb = args.cb;
    const cb_id = crypto.randomUUID();
    args.cb_id = cb_id;
    window._wongus.stream_cbs.set(cb_id, cb);
    return await ipc({ stream_command: args });
  },
};
window.console.log = (...args) => {
  ipc(args.map((a) => a.toString()).join(" "));
};
