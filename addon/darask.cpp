// fcitx5 frontend for darask-core. All composition logic lives in Rust;
// this file only translates fcitx5 events <-> the C ABI in crates/ffi.
#include <fcitx-utils/i18n.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/stringutils.h>
#include <fcitx/addonfactory.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextproperty.h>
#include <fcitx/inputmethodengine.h>
#include <fcitx/inputpanel.h>
#include <fcitx/instance.h>
#include <fcitx/text.h>

#include <cstdlib>
#include <memory>
#include <string>
#include <vector>

extern "C" {
struct DaraskEngine_;
struct DaraskSession_;
DaraskEngine_ *darask_engine_new(const char *data_dir, const char *history_path);
void darask_engine_free(DaraskEngine_ *);
DaraskSession_ *darask_session_new();
void darask_session_free(DaraskSession_ *);
void darask_session_reset(DaraskSession_ *);
int darask_session_key(DaraskEngine_ *, DaraskSession_ *, uint32_t sym, uint32_t mods);
void darask_session_select(DaraskEngine_ *, DaraskSession_ *, uint32_t index);
char *darask_session_state(DaraskEngine_ *, DaraskSession_ *);
void darask_string_free(char *);
}

namespace {

using namespace fcitx;

struct State {
    std::vector<std::pair<bool, std::string>> preedit;
    struct Cand {
        std::string kind, text;
    };
    std::vector<Cand> cands;
    int cursor = -1;
    std::string commit;
};

State parseState(const char *raw) {
    State st;
    for (auto line : stringutils::split(raw, "\n")) {
        auto f = stringutils::split(line, "\t", stringutils::SplitBehavior::KeepEmpty);
        if (f.empty()) continue;
        if (f[0] == "M" && f.size() >= 2) {
            st.commit = f[1];
        } else if (f[0] == "P" && f.size() >= 3) {
            st.preedit.emplace_back(f[1] == "1", f[2]);
        } else if (f[0] == "C" && f.size() >= 3) {
            st.cands.push_back({f[1], f[2]});
        } else if (f[0] == "K" && f.size() >= 2) {
            st.cursor = std::atoi(f[1].c_str());
        }
    }
    return st;
}

class DaraskEngine;

class DaraskSessionProp : public InputContextProperty {
public:
    DaraskSessionProp() : session_(darask_session_new()) {}
    ~DaraskSessionProp() override { darask_session_free(session_); }
    DaraskSession_ *get() { return session_; }

private:
    DaraskSession_ *session_;
};

class DaraskCandidate : public CandidateWord {
public:
    DaraskCandidate(DaraskEngine *engine, size_t index, const std::string &text,
                    const std::string &kind);
    void select(InputContext *ic) const override;

private:
    DaraskEngine *engine_;
    size_t index_;
};

const char *kindComment(const std::string &k) {
    if (k == "en") return "EN";
    if (k == "raw") return "英数";
    if (k == "kata") return "カナ";
    if (k == "part") return "一部カナ";
    if (k == "half") return "半角";
    if (k == "full") return "全角";
    return nullptr;
}

class DaraskEngine : public InputMethodEngineV2 {
public:
    explicit DaraskEngine(Instance *instance)
        : instance_(instance),
          factory_([](InputContext &) { return new DaraskSessionProp(); }) {
        instance_->inputContextManager().registerProperty("darask", &factory_);
        std::string dataDir = DARASK_DATA_DIR;
        if (const char *e = std::getenv("DARASK_DATA")) dataDir = e;
        std::string hist;
        if (const char *xdg = std::getenv("XDG_DATA_HOME")) {
            hist = std::string(xdg) + "/darask-ime/history.tsv";
        } else if (const char *home = std::getenv("HOME")) {
            hist = std::string(home) + "/.local/share/darask-ime/history.tsv";
        }
        engine_ = darask_engine_new(dataDir.c_str(), hist.c_str());
        if (!engine_) FCITX_ERROR() << "darask: cannot open data dir " << dataDir;
    }
    ~DaraskEngine() override {
        if (engine_) darask_engine_free(engine_);
    }

    void keyEvent(const InputMethodEntry &, KeyEvent &event) override {
        if (!engine_ || event.isRelease()) return;
        auto *ic = event.inputContext();
        auto *prop = ic->propertyFor(&factory_);
        const Key &key = event.key();
        uint32_t mods = 0;
        if (key.states().test(KeyState::Shift)) mods |= 1;
        if (key.states().test(KeyState::Ctrl)) mods |= 2;
        if (key.states().test(KeyState::Alt)) mods |= 4;
        auto sym = static_cast<uint32_t>(key.sym());
        // pure modifier presses are never ours
        if (key.isModifier()) return;
        int handled = darask_session_key(engine_, prop->get(), sym, mods);
        update(ic);
        if (handled) event.filterAndAccept();
    }

    void activate(const InputMethodEntry &, InputContextEvent &) override {}
    void deactivate(const InputMethodEntry &, InputContextEvent &event) override {
        resetSession(event.inputContext());
    }
    void reset(const InputMethodEntry &, InputContextEvent &event) override {
        resetSession(event.inputContext());
    }

    void selectCandidate(InputContext *ic, size_t index) {
        auto *prop = ic->propertyFor(&factory_);
        darask_session_select(engine_, prop->get(), static_cast<uint32_t>(index));
        update(ic);
    }

private:
    void resetSession(InputContext *ic) {
        auto *prop = ic->propertyFor(&factory_);
        darask_session_reset(prop->get());
        ic->inputPanel().reset();
        ic->updatePreedit();
        ic->updateUserInterface(UserInterfaceComponent::InputPanel);
    }

    void update(InputContext *ic) {
        auto *prop = ic->propertyFor(&factory_);
        char *raw = darask_session_state(engine_, prop->get());
        State st = parseState(raw);
        darask_string_free(raw);

        if (!st.commit.empty()) ic->commitString(st.commit);

        ic->inputPanel().reset();
        if (!st.preedit.empty()) {
            Text preedit;
            size_t cursor = 0;
            for (auto &[hl, txt] : st.preedit) {
                preedit.append(txt, hl ? TextFormatFlag::HighLight : TextFormatFlag::Underline);
                if (hl) cursor = preedit.toString().size();
            }
            if (cursor == 0) cursor = preedit.toString().size();
            preedit.setCursor(static_cast<int>(cursor));
            if (ic->capabilityFlags().test(CapabilityFlag::Preedit)) {
                ic->inputPanel().setClientPreedit(preedit);
            } else {
                ic->inputPanel().setPreedit(preedit);
            }
        }
        if (!st.cands.empty()) {
            auto list = std::make_unique<CommonCandidateList>();
            list->setPageSize(9);
            list->setLayoutHint(CandidateLayoutHint::Vertical);
            KeyList keys;
            for (char c = '1'; c <= '9'; ++c) keys.emplace_back(Key(static_cast<KeySym>(c)));
            list->setSelectionKey(keys);
            for (size_t i = 0; i < st.cands.size(); ++i) {
                auto w = std::make_unique<DaraskCandidate>(this, i, st.cands[i].text, st.cands[i].kind);
                list->append(std::move(w));
            }
            if (st.cursor >= 0 && st.cursor < static_cast<int>(st.cands.size())) {
                list->setPage(st.cursor / 9);
                list->setCursorIndex(st.cursor % 9);
            }
            ic->inputPanel().setCandidateList(std::move(list));
        }
        ic->updatePreedit();
        ic->updateUserInterface(UserInterfaceComponent::InputPanel);
    }

    Instance *instance_;
    FactoryFor<DaraskSessionProp> factory_;
    DaraskEngine_ *engine_ = nullptr;
};

DaraskCandidate::DaraskCandidate(DaraskEngine *engine, size_t index, const std::string &text,
                                 const std::string &kind)
    : CandidateWord(Text(text)), engine_(engine), index_(index) {
    if (const char *c = kindComment(kind)) setComment(Text(c));
}

void DaraskCandidate::select(InputContext *ic) const { engine_->selectCandidate(ic, index_); }

class DaraskEngineFactory : public AddonFactory {
public:
    AddonInstance *create(AddonManager *manager) override {
        return new DaraskEngine(manager->instance());
    }
};

} // namespace

FCITX_ADDON_FACTORY(DaraskEngineFactory)
