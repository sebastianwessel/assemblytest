
import "wasi";

import {tests,tests2} from './custom';

export * from './env';
export * from './custom';

export function init(configBuffer:ArrayBuffer):void {
  const config = String.UTF8.decode(configBuffer);
  console.log(config);
  console.log("Return value of host function call: "+tests(10).toString());
  console.log("Return value of host function call: "+tests2(20).toString());
}

export function transform(keyBuffer: ArrayBuffer,payloadBuffer: ArrayBuffer): ArrayBuffer {
  const key = String.UTF8.decode(keyBuffer);
  const payload = String.UTF8.decode(payloadBuffer);

  return String.UTF8.encode("transform: "+key+" for payload "+payload);
}
