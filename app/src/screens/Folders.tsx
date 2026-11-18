import { Icon } from '../ui';

/** Папки каждого ящика: «Входящие», «Отправленные», «Черновики» и свои. */
export default function Folders() {
  return (
    <div className="stack">
      <h1 className="screen-title">Папки</h1>
      <div className="empty">
        <div className="empty-icon">
          <Icon name="folder" size={32} />
        </div>
        <p className="muted">Папки появятся, когда подключите ящик.</p>
      </div>
    </div>
  );
}
