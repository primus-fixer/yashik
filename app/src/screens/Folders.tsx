import { FOLDERS, type FolderId } from '../folders';
import { Icon } from '../ui';

/** Папки. На телефоне сверху — общие папки, кроме «Входящих» (они на нижней панели); на компьютере
 * общие папки в меню слева. Ниже — свои папки каждого ящика. */
export default function Folders({ wide, onOpen }: { wide: boolean; onOpen: (id: FolderId) => void }) {
  return (
    <div className="stack">
      <h1 className="screen-title">Папки</h1>
      {!wide && (
        <nav className="folder-list" aria-label="Общие папки">
          {FOLDERS.filter((f) => f.id !== 'inbox').map((f) => (
            <button key={f.id} type="button" className="folder-row" onClick={() => onOpen(f.id)}>
              <Icon name={f.icon} />
              <span>{f.label}</span>
              <Icon name="right" size={18} stroke={2} style={{ color: 'var(--ink3)' }} />
            </button>
          ))}
        </nav>
      )}
      <section className="stack-s">
        <h2 className="sec">Папки ящиков</h2>
        <p className="muted">Появятся, когда подключите ящик: у каждого ящика — свои.</p>
      </section>
    </div>
  );
}
