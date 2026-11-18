// Настройки: вход по PIN-коду, отпечатком или через Windows Hello — у каждого устройства свой.
import type { LockStatus } from '../api';
import { bioLabel, isWindows, type PinMode } from '@primus-fixer/kit/ui';
import { Cut, Icon, NAME, NAME_GEN } from '../ui';

export type Security = {
  lock: LockStatus | null;
  onPin: (mode: PinMode) => void;
  onBio: (on: boolean) => void;
  onLockNow: () => void;
};

const isAndroid = typeof navigator !== 'undefined' && /android/i.test(navigator.userAgent);

export default function SecuritySection({ sec }: { sec: Security }) {
  const lock = sec.lock;
  if (!lock) return null;
  const bio = bioLabel(lock.bio_kind);
  let bioNote: string;
  if (!lock.pin) bioNote = 'Сначала задайте PIN-код — он останется запасным';
  else if (!lock.bio_ready)
    bioNote = isWindows ? 'Windows Hello не настроен: Параметры → Учётные записи → Варианты входа' : isAndroid ? 'На телефоне не добавлен отпечаток пальца' : 'На этом устройстве недоступно';
  else bioNote = isWindows ? `Отпечаток, лицо или PIN Windows — что настроено в Windows Hello. PIN-код «${NAME_GEN}» остаётся запасным` : 'PIN-код остаётся запасным';
  const recover = isWindows ? 'через Windows Hello' : isAndroid ? 'блокировкой экрана телефона' : 'средствами системы';
  return (
    <Cut id="security" title="Вход" hint={!lock.pin ? 'без кода' : lock.bio ? `PIN-код и ${bio.name}` : 'по PIN-коду'}>
      <p className="small muted">
        {lock.pin ? 'PIN-код спрашивается при запуске и после 5 минут в фоне.' : `Сейчас «${NAME}» открывается без кода.`} PIN-код у каждого устройства свой и на другие не переходит.
      </p>
      <div className="sec-actions">
        {!lock.pin ? (
          <button type="button" className="btn primary small" onClick={() => sec.onPin('create')}>
            Задать PIN-код
          </button>
        ) : (
          <>
            <button type="button" className="btn line small" onClick={() => sec.onPin('change')}>
              Сменить PIN
            </button>
            <button type="button" className="btn line small" onClick={() => sec.onPin('off')}>
              Выключить
            </button>
            <button type="button" className="btn line small" onClick={sec.onLockNow}>
              <Icon name="lock" size={18} />
              <span>Заблокировать сейчас</span>
            </button>
          </>
        )}
      </div>
      <label className="cloud-check" style={{ opacity: lock.pin && lock.bio_ready ? 1 : 0.65 }}>
        <input type="checkbox" role="switch" aria-label={bio.title} checked={lock.bio} disabled={!lock.pin || !lock.bio_ready} onChange={(e) => sec.onBio(e.target.checked)} />
        <span className="row-label">
          <span>{bio.title}</span>
          <span className="small muted">{bioNote}</span>
        </span>
      </label>
      {lock.pin && <p className="small muted">Забыли PIN-код — на экране входа нажмите «Забыли PIN?» и подтвердите себя {recover}. Данные останутся.</p>}
    </Cut>
  );
}
