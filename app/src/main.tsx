import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
// Общий вид приложений на kit: шрифты, стили, своя полоса окна; свои стили — после общих.
import '@primus-fixer/kit/ui/fonts';
import '@primus-fixer/kit/ui/styles.css';
import './styles.css';
import { TitleBar, customTitleBar } from '@primus-fixer/kit/ui';
import App from './App';

// Своя полоса окна — сразу, чтобы раскладка не прыгала.
if (customTitleBar) document.documentElement.classList.add('own-titlebar');

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <TitleBar />
    <App />
  </StrictMode>,
);
