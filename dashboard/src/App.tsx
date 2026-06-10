import { BrowserRouter, Routes, Route } from 'react-router-dom';
import Layout from './Layout';
import Overview from './pages/Overview';
import Processes from './pages/Processes';
import Graph from './pages/Graph';
import Memories from './pages/Memories';
import Config from './pages/Settings';
import Logs from './pages/Logs';

export default function App() {
  return (
    <BrowserRouter>
      <Routes>
        <Route element={<Layout />}>
          <Route path="/" element={<Overview />} />
          <Route path="/processes" element={<Processes />} />
          <Route path="/graph" element={<Graph />} />
          <Route path="/memories" element={<Memories />} />
          <Route path="/config" element={<Config />} />
          <Route path="/logs" element={<Logs />} />
        </Route>
      </Routes>
    </BrowserRouter>
  );
}
