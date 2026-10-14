// Runs the effect and the adjustments off the main thread, so a slider stays smooth
// while a three-screen canvas is processed. The pixels arrive and leave transferred.
import { finish } from './engine.js';

self.onmessage = (event) => {
  const { id, buffer, width, height, effect, adjust, regions } = event.data;
  try {
    const data = new Uint8ClampedArray(buffer);
    finish(data, width, height, effect, adjust, regions);
    self.postMessage({ id, buffer }, [buffer]);
  } catch (error) {
    self.postMessage({ id, error: String(error && error.message ? error.message : error) });
  }
};
