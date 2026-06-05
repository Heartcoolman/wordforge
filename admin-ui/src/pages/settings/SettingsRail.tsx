import { For } from 'solid-js';

export interface RailItem {
  key: string;
  label: string;
  group: string;
  icon: () => any;
  count?: number;
  badge?: { text: string; tone: 'warn' | 'err' };
}

interface Props {
  items: RailItem[];
  active: string;
  health: 'ok' | 'warn' | 'error';
  healthPassed: number;
  healthTotal: number;
  rev: string;
  updatedAt?: string;
  onJump: (key: string) => void;
}

const healthClass: Record<string, string> = { ok: '', warn: 'warn', error: 'err' };

export function SettingsRail(props: Props) {
  const groups = () => {
    const out: { label: string; items: RailItem[] }[] = [];
    for (const it of props.items) {
      let g = out.find((x) => x.label === it.group);
      if (!g) { g = { label: it.group, items: [] }; out.push(g); }
      g.items.push(it);
    }
    return out;
  };

  return (
    <aside class="st-rail">
      <div class="head">
        <h3>设置板块</h3>
        <div class={`meta ${healthClass[props.health]}`}>
          <span class="dot" />
          配置健康 · {props.healthPassed} / {props.healthTotal} 通过校验
        </div>
      </div>
      <nav>
        <For each={groups()}>
          {(g) => (
            <>
              <div class="group-label">{g.label}</div>
              <For each={g.items}>
                {(it) => (
                  <a
                    classList={{ 'is-active': props.active === it.key }}
                    onClick={(e) => { e.preventDefault(); props.onJump(it.key); }}
                  >
                    {it.icon()}
                    <span>{it.label}</span>
                    {it.badge
                      ? <span class={`num ${it.badge.tone}`}>{it.badge.text}</span>
                      : it.count !== undefined ? <span class="num">{it.count}</span> : null}
                  </a>
                )}
              </For>
            </>
          )}
        </For>
      </nav>
      <div class="foot">
        <strong>配置版本</strong>
        {props.rev}
        {props.updatedAt ? ` · ${props.updatedAt}` : ''}
      </div>
    </aside>
  );
}
