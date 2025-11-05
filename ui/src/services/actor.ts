export type ActorUpdate<TState> = (state: TState) => TState | Promise<TState>;
export type ActorSubscriber<TState> = (state: TState) => void;

export class Actor<TState> {
  private state: TState;
  private queue: Array<{
    update: ActorUpdate<TState>;
    resolve: (state: TState) => void;
    reject: (error: Error) => void;
  }> = [];
  private flushing = false;
  private subscribers: Set<ActorSubscriber<TState>> = new Set();

  constructor(initialState: TState) {
    this.state = initialState;
  }

  get snapshot(): TState {
    return this.state;
  }

  async send(update: ActorUpdate<TState>): Promise<TState> {
    return new Promise<TState>((resolve, reject) => {
      this.queue.push({ update, resolve, reject });
      if (!this.flushing) {
        this.flushing = true;
        void this.flush();
      }
    });
  }

  subscribe(subscriber: ActorSubscriber<TState>): () => void {
    this.subscribers.add(subscriber);
    queueMicrotask(() => subscriber(this.state));
    return () => {
      this.subscribers.delete(subscriber);
    };
  }

  private async flush(): Promise<void> {
    while (this.queue.length > 0) {
      const { update, resolve, reject } = this.queue.shift()!;
      try {
        const nextState = await update(this.state);
        this.state = nextState;
        this.notify();
        resolve(this.state);
      } catch (error) {
        reject(error as Error);
      }
    }
    this.flushing = false;
  }

  private notify(): void {
    const snapshot = this.state;
    queueMicrotask(() => {
      for (const subscriber of this.subscribers) {
        subscriber(snapshot);
      }
    });
  }
}

export function createActor<TState>(initialState: TState): Actor<TState> {
  return new Actor(initialState);
}

