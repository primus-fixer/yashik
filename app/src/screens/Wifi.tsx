// Связь по Wi-Fi — общий раздел из kit; здесь — слова приложения.
import { WifiSection as Section, type WifiWords } from '@primus-fixer/kit/ui';
import { api, inApp, isMobileApp, message, scanQr } from '../api';
import { FEMININE, NAME } from '../ui';

const WORDS: WifiWords = { name: NAME, feminine: FEMININE, files: 'вложений', where: 'Настройки → Обмен по Wi-Fi', port: 47520 };

/** Настройки → «Обмен по Wi-Fi»: связанные устройства, код связи, кто рядом. */
export default function WifiSection({ onSynced }: { onSynced: () => void }) {
  return <Section api={api} words={WORDS} mobile={isMobileApp} demo={!inApp} scanQr={scanQr} message={message} onSynced={onSynced} />;
}
