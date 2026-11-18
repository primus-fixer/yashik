import { Icon } from '../ui';

/** Входящие: все ящики одним списком. Пока ящиков нет — приглашение подключить первый. `synced` меняется,
 * когда пришло новое с другого устройства, — тогда экран перечитывает данные. */
export default function Inbox({ synced, onConnect }: { synced: number; onConnect: () => void }) {
  return (
    <div className="stack" data-synced={synced}>
      <h1 className="screen-title">Входящие</h1>
      <div className="empty">
        <div className="empty-icon">
          <Icon name="inbox" size={32} />
        </div>
        <p className="muted">Ящиков пока нет. Подключите первый — письма из всех ящиков будут здесь, одним списком.</p>
        <button type="button" className="btn primary" onClick={onConnect}>
          Подключить ящик
        </button>
      </div>
    </div>
  );
}
