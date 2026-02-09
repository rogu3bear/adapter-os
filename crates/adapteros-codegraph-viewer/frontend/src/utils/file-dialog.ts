import { open } from '@tauri-apps/api/dialog';

export const openSqliteDbDialog = async (title: string): Promise<string | null> => {
  try {
    const selected = await open({
      title,
      filters: [
        {
          name: 'SQLite Database',
          extensions: ['db', 'sqlite', 'sqlite3'],
        },
      ],
    });

    return selected && typeof selected === 'string' ? selected : null;
  } catch (error) {
    console.error('Failed to open file dialog:', error);
    return null;
  }
};

