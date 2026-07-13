import { renderDev, RenderOptions } from '@builder.io/qwik/server';
import Root from './root';

export default function (opts: RenderOptions) {
  return renderDev(<Root />, opts);
}
