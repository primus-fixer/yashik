/** Общие папки: у каждого ящика они свои, в «Ящике» — одним списком из всех ящиков. */
export type FolderId = 'inbox' | 'sent' | 'drafts' | 'spam' | 'trash';

/** `role` — назначение папки в ядре; `empty` — пока ящиков нет, `nothing` — ящики есть, а писем нет. */
export type Folder = { id: FolderId; role: string; label: string; icon: string; empty: string; nothing: string };

export const FOLDERS: Folder[] = [
  { id: 'inbox', role: 'inbox', label: 'Входящие', icon: 'inbox', empty: 'Ящиков пока нет. Подключите первый — письма из всех ящиков будут здесь, одним списком.', nothing: 'Во «Входящих» пусто.' },
  { id: 'sent', role: 'sent', label: 'Отправленные', icon: 'send', empty: 'Здесь будут письма, отправленные из всех ящиков.', nothing: 'Отправленных писем нет.' },
  { id: 'drafts', role: 'drafts', label: 'Черновики', icon: 'draft', empty: 'Здесь будут недописанные письма из всех ящиков.', nothing: 'Черновиков нет.' },
  { id: 'spam', role: 'junk', label: 'Спам', icon: 'spam', empty: 'Сюда попадают письма, которые почтовые сервисы сочли спамом.', nothing: 'Спама нет.' },
  { id: 'trash', role: 'trash', label: 'Корзина', icon: 'trash', empty: 'Сюда попадают удалённые письма.', nothing: 'Корзина пуста.' },
];

export const folder = (id: FolderId): Folder => FOLDERS.find((f) => f.id === id) ?? FOLDERS[0];
