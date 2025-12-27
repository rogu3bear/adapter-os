import { useCallback, useEffect, useRef, useState } from 'react';
import { useDemoMode } from './DemoProvider';

interface DemoScriptRunnerOptions {
  enabled?: boolean;
  setInput: (value: string) => void;
  focus?: () => void;
}

export function useDemoScriptRunner({ enabled: enabledOverride, setInput, focus }: DemoScriptRunnerOptions) {
  const { demoScript, enabled: demoEnabled } = useDemoMode();
  const enabled = enabledOverride ?? demoEnabled;
  const [isTyping, setIsTyping] = useState(false);
  const timerRef = useRef<number | null>(null);

  const stop = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    setIsTyping(false);
  }, []);

  const run = useCallback(() => {
    if (!enabled) {
      setInput(demoScript);
      focus?.();
      return;
    }
    stop();
    setInput('');
    setIsTyping(true);
    focus?.();

    let index = 0;
    const chunkSize = 3;

    timerRef.current = window.setInterval(() => {
      index += chunkSize;
      const next = demoScript.slice(0, index);
      setInput(next);
      if (index >= demoScript.length) {
        stop();
      }
    }, 18) as unknown as number;
  }, [demoScript, enabled, focus, setInput, stop]);

  useEffect(() => stop, [stop]);

  return { run, isTyping, demoScript };
}
