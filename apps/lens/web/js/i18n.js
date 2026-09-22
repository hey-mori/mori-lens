// i18n.js - Minimal internationalization module
// Supports English and Korean with deterministic fallback

const translations = {
  en: {
    // App
    'app.title': 'Mori Lens',
    'skip.to.main': 'Skip to main content',

    // Navigation
    'nav.status': 'Status',
    'nav.library': 'Library',
    'nav.review': 'Review',
    'nav.recent': 'Recent',
    'nav.history': 'History',
    'nav.general': 'General',
    'nav.shortcuts': 'Shortcuts',
    'nav.models': 'Models',
    'nav.apps': 'Apps',
    'nav.about': 'About',

    // Status
    'status.title': 'Status',
    'status.loading': 'Loading...',
    'status.health.check': 'Run health check',
    'status.health.running': 'Running...',
    'status.health.failed': 'Health check failed: {error}',

    // Library
    'library.title': 'Library',
    'library.description': 'Items you explicitly saved.',
    'library.search': 'Library search',
    'library.search.placeholder': 'Search saved items...',
    'library.search.button': 'Search',
    'library.export.md': 'Export MD',
    'library.export.json': 'Export JSON',
    'library.export.jsonl': 'Export JSONL',
    'library.import.jsonl': 'Import JSONL',
    'library.filter.type': 'Type:',
    'library.filter.app': 'App:',
    'library.filter.source': 'Source:',
    'library.filter.all': 'All',
    'library.filter.translations': 'Translations',
    'library.filter.analyses': 'Analyses',
    'library.filter.target': 'Target:',
    'library.filter.state': 'State:',
    'library.filter.tag': 'Tag:',
    'library.clear.filters': 'Clear filters',

    // Common
    'common.save': 'Save',
    'common.cancel': 'Cancel',
    'common.delete': 'Delete',
    'common.edit': 'Edit',
    'common.close': 'Close',
    'common.loading': 'Loading...',
    'common.error': 'Error',
    'common.success': 'Success',
    'common.reset': 'Reset',
    'common.add': 'Add',
    'common.refresh': 'Refresh',
    'common.search': 'Search',
    'common.clear': 'Clear',
    'common.all': 'All',
    'common.test': 'Test',

    // Review
    'review.title': 'Review',
    'review.description': 'Items marked for later review. Categorise each as Known or Archive.',
    'review.show.answer': 'Show answer',
    'review.known': 'Known',
    'review.later': 'Keep Later',
    'review.archive': 'Archive',
    'review.undo': 'Undo',
    'review.undo.tooltip': 'Undo last action (Ctrl+Z)',

    // Recent
    'recent.title': 'Recent',
    'recent.description': 'Session only · cleared when Mori Lens quits.',
    'recent.empty': 'No recent cards.',

    // History
    'history.title': 'History',
    'history.description': 'Persistent record of cards when History is enabled. Turn it on in General settings.',
    'history.count': '{count} entries',
    'history.prune': 'Prune old entries',
    'history.loading': 'Loading...',

    // General
    'general.title': 'General',
    'general.language': 'Language',
    'general.dwell': 'Dwell (ms)',
    'general.theme': 'Theme',
    'general.theme.system': 'System',
    'general.theme.forest': 'Forest',
    'general.theme.night-lake': 'Night Lake',
    'general.theme.paper': 'Paper',
    'general.text.size': 'Text size',
    'general.text.size.small': 'Small',
    'general.text.size.normal': 'Normal',
    'general.text.size.large': 'Large',
    'general.text.size.larger': 'Larger',
    'general.translucency': 'Translucency',
    'general.translucency.clear': 'Clear',
    'general.translucency.balanced': 'Balanced',
    'general.translucency.solid': 'Solid',
    'general.ocr.mode': 'OCR mode',
    'general.ocr.mode.off': 'Off',
    'general.ocr.mode.manual': 'Manual (Ctrl+Alt+O)',
    'general.ocr.mode.automatic': 'Automatic',
    'general.profile': 'Profile',
    'general.profile.reader': 'Reader',
    'general.profile.developer': 'Developer',
    'general.profile.exam': 'Exam',
    'general.hover.scope': 'Hover scope',
    'general.hover.scope.sentence': 'Sentence',
    'general.hover.scope.word': 'Word',
    'general.hover.scope.hint': 'What the card answers about. Hold Ctrl to toggle.',
    'general.default.tier': 'Default tier',
    'general.default.tier.allow': 'Allow',
    'general.default.tier.modifier': 'Modifier',
    'general.default.tier.deny': 'Deny',
    'general.history.title': 'History',
    'general.history.enable': 'Enable persistent history',
    'general.history.enable.hint': 'Stores all cards in a database. Disabled by default for privacy.',
    'general.history.retention': 'Retention period',
    'general.history.retention.1day': '1 day',
    'general.history.retention.7days': '7 days',
    'general.history.retention.30days': '30 days (default)',
    'general.history.retention.90days': '90 days',
    'general.history.retention.forever': 'Forever',
    'general.history.store.app': 'Store application name',
    'general.history.store.app.hint': 'Records which app the card came from. Disable for more privacy.',
    'general.history.excluded': 'Excluded applications (comma-separated)',
    'general.history.excluded.placeholder': 'e.g. keepass,password',
    'general.history.excluded.hint': 'Cards from these apps are never recorded in history.',

    // Shortcuts
    'shortcuts.title': 'Shortcuts',
    'shortcuts.description': 'Global keyboard shortcuts. Changes require restart.',
    'shortcuts.defaults': 'Restore defaults',
    'shortcuts.revert': 'Revert',

    // Models
    'models.title': 'Models',
    'models.checking': 'Checking...',
    'models.installed': 'Installed models',
    'models.quick.title': 'Quick translation',
    'models.quick.engine': 'Engine',
    'models.quick.model': 'Model',
    'models.quick.model.auto': 'Auto (best available)',
    'models.quick.endpoint': 'Endpoint',
    'models.deep.title': 'Deep analysis',
    'models.deep.engine': 'Engine',
    'models.deep.model': 'Model (empty = auto)',
    'models.deep.model.placeholder': 'auto',
    'models.deep.endpoint': 'Endpoint',
    'models.test.title': 'Test translation',
    'models.test.sample': '"The output files came back empty."',

    // Apps
    'apps.title': 'Applications',
    'apps.description': 'Per-application rules override the default tier.',
    'apps.process.name': 'Application process name',
    'apps.process.name.placeholder': 'Process name',
    'apps.tier': 'Application tier',
    'apps.ocr': 'Application OCR behavior',
    'apps.ocr.inherit': 'OCR: Inherit global',
    'apps.ocr.off': 'OCR: Off',
    'apps.ocr.manual': 'OCR: Manual',
    'apps.ocr.automatic': 'OCR: Automatic',

    // About
    'about.title': 'About',
    'about.danger.zone': 'Danger zone',
    'about.danger.zone.description': 'Permanently delete all library and history data. This runs a secure wipe (WAL checkpoint, secure_delete, DELETE, VACUUM). Library items and persistent history are removed. saved.md and exports you have made are not affected. This does not reach filesystem snapshots, drive controller remapped blocks, or backups.',
    'about.danger.zone.button': 'Delete all library and history data',
  },

  ko: {
    // App
    'app.title': '모리 렌즈',
    'skip.to.main': '본문으로 건너뛰기',

    // Navigation
    'nav.status': '상태',
    'nav.library': '라이브러리',
    'nav.review': '검토',
    'nav.recent': '최근',
    'nav.history': '기록',
    'nav.general': '일반',
    'nav.shortcuts': '단축키',
    'nav.models': '모델',
    'nav.apps': '앱',
    'nav.about': '정보',

    // Status
    'status.title': '상태',
    'status.loading': '로딩 중...',
    'status.health.check': '헬스체크 실행',
    'status.health.running': '실행 중...',
    'status.health.failed': '헬스체크 실패: {error}',

    // Library
    'library.title': '라이브러리',
    'library.description': '명시적으로 저장한 항목들.',
    'library.search': '라이브러리 검색',
    'library.search.placeholder': '저장된 항목 검색...',
    'library.search.button': '검색',
    'library.export.md': 'MD 내보내기',
    'library.export.json': 'JSON 내보내기',
    'library.export.jsonl': 'JSONL 내보내기',
    'library.import.jsonl': 'JSONL 가져오기',
    'library.filter.type': '유형:',
    'library.filter.app': '앱:',
    'library.filter.source': '소스:',
    'library.filter.all': '전체',
    'library.filter.translations': '번역',
    'library.filter.analyses': '분석',
    'library.filter.target': '대상:',
    'library.filter.state': '상태:',
    'library.filter.tag': '태그:',
    'library.clear.filters': '필터 지우기',

    // Common
    'common.save': '저장',
    'common.cancel': '취소',
    'common.delete': '삭제',
    'common.edit': '편집',
    'common.close': '닫기',
    'common.loading': '로딩 중...',
    'common.error': '오류',
    'common.success': '성공',
    'common.reset': '초기화',
    'common.add': '추가',
    'common.refresh': '새로고침',
    'common.search': '검색',
    'common.clear': '지우기',
    'common.all': '전체',
    'common.test': '테스트',

    // Review
    'review.title': '검토',
    'review.description': '나중에 검토할 항목들. 각각 알음 또는 보관으로 분류하세요.',
    'review.show.answer': '답변 보기',
    'review.known': '알음',
    'review.later': '나중에',
    'review.archive': '보관',
    'review.undo': '실행 취소',
    'review.undo.tooltip': '마지막 작업 되돌리기 (Ctrl+Z)',

    // Recent
    'recent.title': '최근',
    'recent.description': '세션 전용 · 모리 렌즈 종료 시 초기화됩니다.',
    'recent.empty': '최근 카드 없음.',

    // History
    'history.title': '기록',
    'history.description': '기록이 활성화된 경우 카드의 영구 기록입니다. 일반 설정에서 켜세요.',
    'history.count': '{count}개 항목',
    'history.prune': '오래된 항목 정리',
    'history.loading': '로딩 중...',

    // General
    'general.title': '일반',
    'general.language': '언어',
    'general.dwell': '머무는 시간 (ms)',
    'general.theme': '테마',
    'general.theme.system': '시스템',
    'general.theme.forest': '숲',
    'general.theme.night-lake': '밤 호수',
    'general.theme.paper': '종이',
    'general.text.size': '글자 크기',
    'general.text.size.small': '작게',
    'general.text.size.normal': '보통',
    'general.text.size.large': '크게',
    'general.text.size.larger': '더 크게',
    'general.translucency': '반투명도',
    'general.translucency.clear': '투명',
    'general.translucency.balanced': '적당',
    'general.translucency.solid': '불투명',
    'general.ocr.mode': 'OCR 모드',
    'general.ocr.mode.off': '끔',
    'general.ocr.mode.manual': '수동 (Ctrl+Alt+O)',
    'general.ocr.mode.automatic': '자동',
    'general.profile': '프로필',
    'general.profile.reader': '리더',
    'general.profile.developer': '개발자',
    'general.profile.exam': '시험',
    'general.hover.scope': '호버 범위',
    'general.hover.scope.sentence': '문장',
    'general.hover.scope.word': '단어',
    'general.hover.scope.hint': '카드가 답하는 범위입니다. Ctrl을 눌러 전환하세요.',
    'general.default.tier': '기본 등급',
    'general.default.tier.allow': '허용',
    'general.default.tier.modifier': '수정자',
    'general.default.tier.deny': '거부',
    'general.history.title': '기록',
    'general.history.enable': '영구 기록 사용',
    'general.history.enable.hint': '모든 카드를 데이터베이스에 저장합니다. 개인정보 보호를 위해 기본적으로 꺼져 있습니다.',
    'general.history.retention': '보존 기간',
    'general.history.retention.1day': '1일',
    'general.history.retention.7days': '7일',
    'general.history.retention.30days': '30일 (기본값)',
    'general.history.retention.90days': '90일',
    'general.history.retention.forever': '영구',
    'general.history.store.app': '애플리케이션 이름 저장',
    'general.history.store.app.hint': '카드가 나온 앱을 기록합니다. 더 많은 개인정보 보호를 위해 끄세요.',
    'general.history.excluded': '제외된 애플리케이션 (쉼표로 구분)',
    'general.history.excluded.placeholder': '예: keepass,password',
    'general.history.excluded.hint': '이 앱의 카드는 기록에 저장되지 않습니다.',

    // Shortcuts
    'shortcuts.title': '단축키',
    'shortcuts.description': '글로벌 키보드 단축키. 변경 시 재시작이 필요합니다.',
    'shortcuts.defaults': '기본값 복원',
    'shortcuts.revert': '되돌리기',

    // Models
    'models.title': '모델',
    'models.checking': '확인 중...',
    'models.installed': '설치된 모델',
    'models.quick.title': '빠른 번역',
    'models.quick.engine': '엔진',
    'models.quick.model': '모델',
    'models.quick.model.auto': '자동 (최적)',
    'models.quick.endpoint': '엔드포인트',
    'models.deep.title': '심층 분석',
    'models.deep.engine': '엔진',
    'models.deep.model': '모델 (비워두면 자동)',
    'models.deep.model.placeholder': '자동',
    'models.deep.endpoint': '엔드포인트',
    'models.test.title': '번역 테스트',
    'models.test.sample': '"출력 파일이 비어 있게 돌아왔다."',

    // Apps
    'apps.title': '애플리케이션',
    'apps.description': '앱별 규칙이 기본 등급을 재정의합니다.',
    'apps.process.name': '애플리케이션 프로세스 이름',
    'apps.process.name.placeholder': '프로세스 이름',
    'apps.tier': '애플리케이션 등급',
    'apps.ocr': '애플리케이션 OCR 동작',
    'apps.ocr.inherit': 'OCR: 전역 설정 따르기',
    'apps.ocr.off': 'OCR: 끔',
    'apps.ocr.manual': 'OCR: 수동',
    'apps.ocr.automatic': 'OCR: 자동',

    // About
    'about.title': '정보',
    'about.danger.zone': '위험 구역',
    'about.danger.zone.description': '모든 라이브러리 및 기록 데이터를 영구 삭제합니다. 안전한 삭제(WAL 체크포인트, secure_delete, DELETE, VACUUM)를 실행합니다. 라이브러리 항목과 영구 기록이 제거됩니다. saved.md와 만든 내보내기는 영향을 받지 않습니다. 파일시스템 스냅샷, 드라이브 컨트롤러 매핑 블록, 백업에는 영향을 주지 않습니다.',
    'about.danger.zone.button': '모든 라이브러리 및 기록 데이터 삭제',
  },
};

let currentLocale = 'en';

// Detect locale from system or storage
function detectLocale() {
  // Check localStorage first
  const stored = localStorage.getItem('lens-locale');
  if (stored && (stored === 'en' || stored === 'ko')) {
    return stored;
  }

  // Fall back to system locale
  const systemLocale = navigator.language || navigator.userLanguage;
  if (systemLocale && systemLocale.startsWith('ko')) {
    return 'ko';
  }

  return 'en';
}

// Initialize i18n
export function initI18n() {
  currentLocale = detectLocale();
}

// Get current locale
export function getLocale() {
  return currentLocale;
}

// Set locale and persist
export function setLocale(locale) {
  if (locale !== 'en' && locale !== 'ko') {
    console.warn(`Unsupported locale: ${locale}, falling back to English`);
    locale = 'en';
  }
  currentLocale = locale;
  localStorage.setItem('lens-locale', locale);

  // Trigger re-render
  document.dispatchEvent(new CustomEvent('locale-changed', { detail: { locale } }));
}

// Translate key with optional interpolation
export function t(key, params = {}) {
  const dict = translations[currentLocale] || translations.en;
  let text = dict[key];

  // Fallback to English if key not found in current locale
  if (text === undefined) {
    text = translations.en[key];
  }

  // If still not found, return the key itself
  if (text === undefined) {
    console.warn(`Missing translation key: ${key}`);
    return key;
  }

  // Simple interpolation: replace {param} with value
  return text.replace(/\{(\w+)\}/g, (match, paramKey) => {
    return params[paramKey] !== undefined ? params[paramKey] : match;
  });
}

// Check if a key exists
export function hasKey(key) {
  return translations[currentLocale]?.[key] !== undefined ||
         translations.en[key] !== undefined;
}

// Get all keys for a locale (useful for testing)
export function getKeys(locale = currentLocale) {
  return Object.keys(translations[locale] || {});
}
