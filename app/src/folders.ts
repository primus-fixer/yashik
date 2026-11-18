/** Общие папки: у каждого ящика они свои, в «Ящике» — одним списком из всех ящиков. */
export type FolderId = 'inbox' | 'sent' | 'drafts' | 'spam' | 'trash';

export type Folder = { id: FolderId; label: string; icon: string; empty: string };

export const FOLDERS: Folder[] = [
  { id: 'inbox', label: 'Входящие', icon: 'inbox', empty: 'Ящиков пока нет. Подключите первый — письма из всех ящиков будут здесь, одним списком.' },
  { id: 'sent', label: 'Отправленные', icon: 'send', empty: 'Здесь будут письма, отправленные из всех ящиков.' },
  { id: 'drafts', label: 'Черновики', icon: 'draft', empty: 'Здесь будут недописанные письма из всех ящиков.' },
  { id: 'spam', label: 'Спам', icon: 'spam', empty: 'Сюда попадают письма, которые почтовые сервисы сочли спамом.' },
  { id: 'trash', label: 'Корзина', icon: 'trash', empty: 'Сюда попадают удалённые письма.' },
];

export const folder = (id: FolderId): Folder => FOLDERS.find((f) => f.id === id) ?? FOLDERS[0];
