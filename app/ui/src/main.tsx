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

installKeyGuard();
window.addEventListener('error', (e) => api.log('error', `${e.message} @ ${e.filename}:${e.lineno}`));
window.addEventListener('unhandledrejection', (e) => api.log('error', `unhandled: ${String(e.reason)}`));

createRoot(document.getElementById('root') as HTMLElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
