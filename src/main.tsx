import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { MailpitToastWindow } from './components/MailpitToastWindow';
import './styles/index.css';

const params = new URLSearchParams(window.location.search);
const isMailToast = params.get('mode') === 'mail-toast';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    {isMailToast ? <MailpitToastWindow /> : <App />}
  </React.StrictMode>,
);
