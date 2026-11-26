import Html from '@kitajs/html';
import type { EarnedAchievement } from '@klipt/shared';
import { esc as e } from './escape.js';

/** Every badge someone earned, newest first; the name and description show on hover. */
export function BadgeStrip(props: { earned: EarnedAchievement[] }): JSX.Element {
  if (props.earned.length === 0) return <></>;
  return (
    <ul class="badge-strip" aria-label="Achievements">
      {props.earned.map((a) => (
        <li>
          <span class="badge-medal" title={e(`${a.name}: ${a.description}`)}>
            <span aria-hidden="true">{e(a.icon)}</span>
            <span class="badge-name">{e(a.name)}</span>
          </span>
        </li>
      ))}
    </ul>
  );
}
