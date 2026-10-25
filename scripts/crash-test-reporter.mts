/** Serialize Node's own terminal test events, without inferring success from exit status. */
export default async function* reporter(events: AsyncIterable<{ type: string; data: unknown }>): AsyncGenerator<string> {
  for await (const event of events) {
    if (event.type === 'test:pass' || event.type === 'test:fail') yield JSON.stringify(event) + '\n';
  }
}
