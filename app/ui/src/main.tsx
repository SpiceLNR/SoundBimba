// SPDX-License-Identifier: AGPL-3.0-or-later
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@xyflow/react/dist/style.css';
import './fonts';
import './theme';
import './app.css';
import { api } from './api';
import App from './App';
import { installKeyGuard } from './keyguard';
import PanelApp from './PanelApp';

installKeyGuard();
window.addEventListener('error', (e) => api.log('error', `${e.message} @ ${e.filename}:${e.lineno}`));
window.addEventListener('unhandledrejection', (e) => api.log('error', `unhandled: ${String(e.reason)}`));

createRoot(document.getElementById('root') as HTMLElement).render(
  <StrictMode>
    {/* Одно приложение — два окна: основное и кнопочная панель (адрес #buttons). */}
    {window.location.hash === '#buttons' ? <PanelApp /> : <App />}
  </StrictMode>,
);
