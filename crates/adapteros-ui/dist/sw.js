// PRD-UI-170: Service Worker for adapterOS UI
// Caches WASM, CSS, and static assets for faster subsequent loads

const CACHE_VERSION = 'aos-__AOS_BUILD_ID__';
const STATIC_CACHE = `${CACHE_VERSION}-static`;
const WASM_CACHE = `${CACHE_VERSION}-wasm`;

// Assets to cache on install
const PRECACHE_ASSETS = [
  '/',
  '/index.html'
];

// Install event - precache critical assets
self.addEventListener('install', (event) => {
  console.log('[SW] Installing...');
  event.waitUntil(
    caches.open(STATIC_CACHE)
      .then((cache) => {
        console.log('[SW] Precaching static assets');
        return cache.addAll(PRECACHE_ASSETS);
      })
      .then(() => self.skipWaiting())
  );
});

// Activate event - clean up old caches
self.addEventListener('activate', (event) => {
  console.log('[SW] Activating...');
  event.waitUntil(
    caches.keys()
      .then((cacheNames) => {
        return Promise.all(
          cacheNames
            .filter((name) => name.startsWith('aos-') && name !== STATIC_CACHE && name !== WASM_CACHE)
            .map((name) => {
              console.log('[SW] Deleting old cache:', name);
              return caches.delete(name);
            })
        );
      })
      .then(() => self.clients.claim())
  );
});

// Fetch event - cache-first for WASM/CSS, network-first for API
self.addEventListener('fetch', (event) => {
  const url = new URL(event.request.url);

  // Skip non-GET requests
  if (event.request.method !== 'GET') {
    return;
  }

  // Skip API requests - always fetch fresh
  if (url.pathname.startsWith('/v1/') || 
      url.pathname.startsWith('/api/') || 
      url.pathname === '/healthz' || 
      url.pathname === '/readyz') {
    return;
  }

  // WASM files - cache-first with long-term caching
  if (url.pathname.endsWith('.wasm')) {
    event.respondWith(
      caches.open(WASM_CACHE)
        .then((cache) => {
          return cache.match(event.request)
            .then((cachedResponse) => {
              if (cachedResponse) {
                console.log('[SW] WASM cache hit:', url.pathname);
                return cachedResponse;
              }

              console.log('[SW] WASM cache miss, fetching:', url.pathname);
              return fetch(event.request)
                .then((response) => {
                  if (response.ok) {
                    cache.put(event.request, response.clone());
                  }
                  return response;
                });
            });
        })
    );
    return;
  }

  // CSS files - cache-first with background update
  if (url.pathname.endsWith('.css')) {
    event.respondWith(
      caches.open(STATIC_CACHE)
        .then((cache) => {
          return cache.match(event.request)
            .then((cachedResponse) => {
              const fetchPromise = fetch(event.request)
                .then((response) => {
                  if (response.ok) {
                    cache.put(event.request, response.clone());
                  }
                  return response;
                });

              // Return cached version immediately, update in background
              return cachedResponse || fetchPromise;
            });
        })
    );
    return;
  }

  // JS files - cache-first (includes trunk-generated JS)
  if (url.pathname.endsWith('.js') && !url.pathname.includes('sw.js')) {
    event.respondWith(
      caches.open(STATIC_CACHE)
        .then((cache) => {
          return cache.match(event.request)
            .then((cachedResponse) => {
              if (cachedResponse) {
                return cachedResponse;
              }

              return fetch(event.request)
                .then((response) => {
                  if (response.ok) {
                    cache.put(event.request, response.clone());
                  }
                  return response;
                });
            });
        })
    );
    return;
  }

  // HTML - network-first to always get fresh content
  if (url.pathname === '/' || url.pathname.endsWith('.html')) {
    event.respondWith(
      fetch(event.request)
        .then((response) => {
          if (response.ok) {
            caches.open(STATIC_CACHE)
              .then((cache) => cache.put(event.request, response.clone()));
          }
          return response;
        })
        .catch(() => {
          return caches.match(event.request);
        })
    );
    return;
  }
});
