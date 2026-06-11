/**
 * 09 — Form validation (React Hook Form)
 *
 * React Hook Form works in the terminal through `Controller` (or `useController`),
 * which bridges atto-ui's controlled widgets — never `register()`, which needs a
 * DOM input ref. Wire `field.value`/`field.onChange` to a TextBox, validate with
 * RHF rules, and submit from a Button via `handleSubmit`.
 * Run interactively:  npm run form
 * Headless smoke:      ATTO_UI_EXAMPLE_HEADLESS=1 npm run form
 */
import { useState } from 'react'
import { Controller, useForm } from 'react-hook-form'
import { Button, Text, TextBox, VStack, Window } from '@atto-ui/react'

import { hasText, sendChar, sendKey, startDemo, waitFor } from './_runtime'

type SignUp = { name: string; email: string }

const EMAIL = /^[^@\s]+@[^@\s]+$/

function App() {
  const [status, setStatus] = useState('editing')
  const {
    control,
    handleSubmit,
    formState: { errors },
  } = useForm<SignUp>({ mode: 'onChange', defaultValues: { name: '', email: '' } })

  const onValid = (data: SignUp) => setStatus(`submitted: ${data.name}`)

  return (
    <Window title="Sign up" rect={[2, 1, 48, 16]}>
      <VStack spacing={1} padding={1}>
        <Controller
          name="name"
          control={control}
          rules={{ required: 'Name is required' }}
          render={({ field }) => (
            <TextBox title="Name" value={field.value} onChange={field.onChange} />
          )}
        />
        {errors.name && <Text>{`! ${errors.name.message}`}</Text>}

        <Controller
          name="email"
          control={control}
          rules={{
            required: 'Email is required',
            pattern: { value: EMAIL, message: 'Invalid email' },
          }}
          render={({ field }) => (
            <TextBox title="Email" value={field.value} onChange={field.onChange} />
          )}
        />
        {errors.email && <Text>{`! ${errors.email.message}`}</Text>}

        <Button onClick={() => void handleSubmit(onValid)()}>Submit</Button>
        <Text>{`Status: ${status}`}</Text>
      </VStack>
    </Window>
  )
}

startDemo(<App />, {
  singleWindow: false,
  idPrefix: 'form',
  async headlessProbe(handle) {
    const windowId = handle.windowIds()[0]!
    await waitFor(() => hasText(handle, 'Status: editing'), 'initial form')

    // Name field is focused first; type a valid name.
    for (const ch of 'Ada') sendChar(handle, windowId, ch)

    // Tab to the email field and type an invalid value — onChange validation fires.
    sendKey(handle, windowId, 'tab')
    for (const ch of 'bad') sendChar(handle, windowId, ch)
    await waitFor(() => hasText(handle, 'Invalid email'), 'pattern error while typing')

    // Complete the address so it matches; the error clears.
    for (const ch of '@x.io') sendChar(handle, windowId, ch)
    await waitFor(() => !hasText(handle, 'Invalid email'), 'pattern error cleared')

    // Tab to Submit and run validation; the valid form submits.
    sendKey(handle, windowId, 'tab')
    sendKey(handle, windowId, 'enter')
    await waitFor(() => hasText(handle, 'Status: submitted: Ada'), 'submit succeeded')
  },
})
