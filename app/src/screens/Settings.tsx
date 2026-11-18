import { useEffect, useState } from 'react';
import { SCHEMES, type ThemeMode } from '@primus-fixer/kit/ui';
import type { Nav } from '../App';
import { api, inApp, message, type About } from '../api';
import { Cut, NAME, Seg } from '../ui';
import AccountsSection from './Accounts';
import CloudSection from './Cloud';
import SecuritySection, { type Security } from './Security';
import WifiSection from './Wifi';

type Props = {
  scheme: string;
  setScheme: (s: string) => void;
  mode: ThemeMode;
  setMode: (m: ThemeMode) => void;
  nav: Nav;
  /** Вход по PIN-коду и отпечатку. */
  security: Security;
  /** Пришли записи с другого устройства — перечитать. */
  onSynced: () => void;
  /** Счётчик обновлений с других устройств: разделы перечитывают данные. */
  synced: number;
  /** Ящики подключили или отключили. */
  onAccounts: () => void;
};

const METHOD: Record<string, string> = {
  dpapi: 'ключ базы защищён учётной записью Windows',
  'android-keystore': 'ключ базы — в хранилище ключей Android',
  file: 'ключ базы — в файле рядом с базой (режим разработки)',
  demo: 'демо в браузере: данные не сохраняются',
};

/** Настройки — один экран из разделов под катом. */
export default function Settings({ scheme, setScheme, mode, setMode, nav, security, onSynced, synced, onAccounts }: Props) {
  const [about, setAbout] = useState<About | null>(null);
  const dark = mode === 'dark' || (mode === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches);

  useEffect(() => {
    api.about().then(setAbout, (e) => nav.toast(message(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="stack settings">
      <h1 className="screen-title">Настройки</h1>

      <AccountsSection onChange={onAccounts} toast={nav.toast} synced={synced} />

      <Cut id="look" title="Вид" hint={`${SCHEMES[scheme]?.name ?? ''}, ${mode === 'dark' ? 'тёмная' : mode === 'light' ? 'светлая' : 'как в системе'}`}>
        <div className="schemes" role="group" aria-label="Расцветка">
          {Object.entries(SCHEMES).map(([id, s]) => {
            const t = dark ? s.dark : s.light;
            return (
              <button key={id} type="button" className={`scheme${id === scheme ? ' on' : ''}`} aria-pressed={id === scheme} onClick={() => setScheme(id)}>
                <span className="swatch" style={{ background: `linear-gradient(90deg, ${t.own} 50%, ${t.work} 50%)` }} />
                <span>{s.name}</span>
              </button>
            );
          })}
        </div>
        <Seg
          cols
          small
          label="Тема"
          value={mode}
          options={[
            ['light', 'Светлая'],
            ['dark', 'Тёмная'],
            ['system', 'Как в системе'],
          ]}
          onChange={setMode}
        />
      </Cut>

      <SecuritySection sec={security} />

      <WifiSection onSynced={onSynced} />

      <CloudSection />

      <Cut id="data" title="Данные">
        <p className="small muted">
          Ящики и пароли — в зашифрованной базе на устройстве{about ? `; ${METHOD[about.key_method] ?? about.key_method}` : ''}. На другие ваши устройства они уходят только по Wi-Fi или через облако — зашифрованными.
        </p>
        {!inApp && <p className="small warn-text">Это демо в браузере: изменения пропадут после перезагрузки.</p>}
      </Cut>

      <p className="small muted3">
        {NAME} {__APP_VERSION__}
      </p>
    </div>
  );
}
