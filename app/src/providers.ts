import type { Provider } from './api';

/** Почтовые службы для выбора при подключении ящика. */
export const PROVIDERS: [Provider, string][] = [
  ['yandex', 'Яндекс'],
  ['gmail', 'Gmail'],
  ['mailru', 'Mail.ru'],
  ['other', 'Другая'],
];

// Те же домены, что в crates/yashik-core/src/account.rs (Provider::of).
const DOMAINS: Record<Exclude<Provider, 'other'>, string[]> = {
  yandex: ['yandex.ru', 'ya.ru', 'yandex.com', 'yandex.by', 'yandex.kz', 'yandex.ua', 'yandex.uz', 'yandex.com.tr', 'narod.ru'],
  gmail: ['gmail.com', 'googlemail.com'],
  mailru: ['mail.ru', 'inbox.ru', 'list.ru', 'bk.ru', 'internet.ru', 'xmail.ru'],
};

/** Служба по адресу; свой домен (в том числе рабочий на Яндекс 360) — «другая», пока не выбрали вручную. */
export function providerOf(email: string): Provider {
  const domain = email.split('@').pop()?.trim().toLowerCase() ?? '';
  for (const [p, list] of Object.entries(DOMAINS) as [Provider, string[]][]) if (list.includes(domain)) return p;
  return 'other';
}

/** Где взять пароль приложения — подсказка под выбором службы. */
export const PASSWORD_HINT: Record<Provider, string> = {
  yandex:
    'Нужен пароль приложения для Почты, а не пароль от Яндекс ID: Безопасность → Пароли приложений → Почта. В Яндекс Почте должен быть включён доступ почтовых программ: Все настройки → Почтовые программы. Подходит и рабочая почта на Яндекс 360.',
  gmail: 'Нужен пароль приложения: аккаунт Google → Безопасность → Пароли приложений (пункт появляется при включённой двухэтапной проверке). Обычный пароль Google не подойдёт.',
  mailru: 'Нужен пароль для внешнего приложения: Настройки → Безопасность → Пароли для внешних приложений.',
  other: 'Нужны серверы IMAP (порт 993) и SMTP (порт 465) с шифрованием. Если почта со своим доменом работает на Яндекс 360, выберите «Яндекс».',
};
