// Облако — общий раздел из kit; здесь — слова приложения.
import { CloudSection as Section, type CloudWords } from '@primus-fixer/kit/ui';
import { api, inApp, message } from '../api';
import { FEMININE, NAME, NAME_GEN } from '../ui';

const WORDS: CloudWords = {
  name: NAME,
  nameGen: NAME_GEN,
  feminine: FEMININE,
  what: 'Записи',
  blobs: { many: 'вложений', upload: 'Отправляю вложения', download: 'Получаю вложения', waiting: ['вложение придёт', 'вложения придут', 'вложений придут'] },
};

/** Настройки → «Облако»: Яндекс Диск по паролю приложения или свой сервер WebDAV. */
export default function CloudSection() {
  return <Section api={api} words={WORDS} message={message} demo={!inApp} />;
}
