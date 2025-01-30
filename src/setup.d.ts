declare interface Window {
  _wongus: {
    stream_cbs: Map<number, (line: string) => void>;
    responses: Map<number, (body: any) => void>;
    external_ipc: (id: number, args: any) => void;
  };
  ipc: {
    postMessage: (message: string) => void;
  };
}
