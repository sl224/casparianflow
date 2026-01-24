import { Routes, Route, Navigate } from 'react-router-dom'
import Sidebar from './components/Sidebar'
import Home from './screens/Home'
import Approvals from './screens/Approvals'
import Query from './screens/Query'
import Sessions from './screens/Sessions'
import SessionDetail from './screens/SessionDetail'
import Jobs from './screens/Jobs'
import Discover from './screens/Discover'
import Parsers from './screens/Parsers'
import Settings from './screens/Settings'

function App() {
  return (
    <div className="app-layout">
      <Sidebar />
      <Routes>
        <Route path="/" element={<Navigate to="/home" replace />} />
        <Route path="/home" element={<Home />} />
        <Route path="/discover" element={<Discover />} />
        <Route path="/sessions" element={<Sessions />} />
        <Route path="/sessions/:sessionId" element={<SessionDetail />} />
        <Route path="/parsers" element={<Parsers />} />
        <Route path="/jobs" element={<Jobs />} />
        <Route path="/approvals" element={<Approvals />} />
        <Route path="/query" element={<Query />} />
        <Route path="/settings" element={<Settings />} />
      </Routes>
    </div>
  )
}

export default App
