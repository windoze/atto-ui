/**
 * 08 — Routing (React Router)
 *
 * React Router works in the terminal through its DOM-free `MemoryRouter`.
 * Navigate from atto-ui Buttons with `useNavigate()` and render atto-ui
 * components inside each route — never `BrowserRouter` or the DOM `<Link>`.
 * Run interactively:  npm run router   (Enter/Space on the focused button)
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run router
 */
import { MemoryRouter, Route, Routes, useLocation, useNavigate } from 'react-router'
import { Button, Divider, Text, VStack, Window } from '@atto-ui/react'

import { hasText, sendKey, startDemo, waitFor } from './_runtime'

function Breadcrumb() {
  const { pathname } = useLocation()
  return <Text>{`Path: ${pathname}`}</Text>
}

function Home() {
  const navigate = useNavigate()
  return (
    <VStack spacing={1} padding={1}>
      <Text>Home — pick a destination.</Text>
      <Button onClick={() => navigate('/about')}>Open About</Button>
    </VStack>
  )
}

function About() {
  const navigate = useNavigate()
  return (
    <VStack spacing={1} padding={1}>
      <Text>About — atto-ui driven by React Router.</Text>
      <Button onClick={() => navigate('/settings')}>Open Settings</Button>
    </VStack>
  )
}

function Settings() {
  const navigate = useNavigate()
  return (
    <VStack spacing={1} padding={1}>
      <Text>Settings — end of the line.</Text>
      <Button onClick={() => navigate('/')}>Back home</Button>
    </VStack>
  )
}

function App() {
  return (
    <Window title="Router" rect={[2, 1, 46, 12]}>
      <MemoryRouter>
        <VStack padding={1}>
          <Breadcrumb />
          <Divider />
          <Routes>
            <Route path="/" element={<Home />} />
            <Route path="/about" element={<About />} />
            <Route path="/settings" element={<Settings />} />
          </Routes>
        </VStack>
      </MemoryRouter>
    </Window>
  )
}

startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'router',
  async headlessProbe(handle) {
    const windowId = handle.windowIds()[0]!
    await waitFor(() => hasText(handle, 'Home — pick a destination.'), 'home route')
    sendKey(handle, windowId, 'enter')
    await waitFor(() => hasText(handle, 'Path: /about'), 'about route after navigate')
    sendKey(handle, windowId, 'enter')
    await waitFor(() => hasText(handle, 'Path: /settings'), 'settings route after navigate')
    sendKey(handle, windowId, 'enter')
    await waitFor(() => hasText(handle, 'Home — pick a destination.'), 'home route after navigate back')
  },
})
