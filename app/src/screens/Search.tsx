import { useState } from 'react';
import { Icon } from '../ui';

/** Поиск по всей почте: на устройстве и на сервере, с русскими словоформами. */
export default function Search() {
  const [query, setQuery] = useState('');
  return (
    <div className="stack">
      <h1 className="screen-title">Поиск</h1>
      <input className="field" type="search" placeholder="Слова, отправитель, тема" aria-label="Что искать" value={query} onChange={(e) => setQuery(e.target.value)} />
      <div className="empty">
        <div className="empty-icon">
          <Icon name="search" size={32} />
        </div>
        <p className="muted">Искать пока негде: подключите ящик.</p>
      </div>
    </div>
  );
}
