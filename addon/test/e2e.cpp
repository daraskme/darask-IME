// End-to-end test: loads the real darask addon into a real fcitx5 Instance and types keys.
#include <fcitx-utils/eventdispatcher.h>
#include <fcitx-utils/key.h>
#include <fcitx-utils/log.h>
#include <fcitx/addonmanager.h>
#include <fcitx/inputmethodgroup.h>
#include <fcitx/inputmethodmanager.h>
#include <fcitx/instance.h>
#include <testfrontend_public.h>
#include <iostream>

using namespace fcitx;

static void typeKeys(AddonInstance *fe, ICUUID uuid, const std::string &s) {
    for (char c : s) fe->call<ITestFrontend::sendKeyEvent>(uuid, Key(static_cast<KeySym>(c)), false);
}
static void press(AddonInstance *fe, ICUUID uuid, const char *k) {
    fe->call<ITestFrontend::sendKeyEvent>(uuid, Key(k), false);
}

int main() {
    char arg0[] = "e2e", arg1[] = "--disable=all", arg2[] = "--enable=keyboard,testfrontend,darask";
    char *argv[] = {arg0, arg1, arg2};
    Instance instance(3, argv);
    instance.addonManager().registerDefaultLoader(nullptr);
    int failures = 0;
    auto &d = instance.eventDispatcher();
    d.schedule([&]() {
        auto &imm = instance.inputMethodManager();
        InputMethodGroup group("Default");
        group.inputMethodList().emplace_back("darask");
        group.setDefaultInputMethod("");
        imm.setGroup(std::move(group));
        auto *fe = instance.addonManager().addon("testfrontend");
        if (!fe) { std::cerr << "FAIL: testfrontend missing\n"; failures++; instance.exit(); return; }
        auto uuid = fe->call<ITestFrontend::createInputContext>("e2e");
        auto *ic = instance.inputContextManager().findByUUID(uuid);
        ic->focusIn();
        std::cerr << "current IM: " << instance.inputMethod(ic) << "\n";
        if (instance.inputMethod(ic) != "darask") { std::cerr << "FAIL: darask not active\n"; failures++; }

        fe->call<ITestFrontend::pushCommitExpectation>("今日はいい天気ですね");
        typeKeys(fe, uuid, "kyouhaiitenkidesune");
        press(fe, uuid, "space");
        press(fe, uuid, "Return");

        fe->call<ITestFrontend::pushCommitExpectation>("エクステンション");
        typeKeys(fe, uuid, "ekusutensyon");
        press(fe, uuid, "space");
        press(fe, uuid, "Return");

        fe->call<ITestFrontend::pushCommitExpectation>("extension");
        typeKeys(fe, uuid, "ekusutensyon");
        press(fe, uuid, "space");
        press(fe, uuid, "Down");
        press(fe, uuid, "Return");

        fe->call<ITestFrontend::pushCommitExpectation>("dockerをインストールする");
        typeKeys(fe, uuid, "dockerwoinsutorusuru");
        press(fe, uuid, "space");
        press(fe, uuid, "Return");

        fe->call<ITestFrontend::pushCommitExpectation>("にほんご");
        typeKeys(fe, uuid, "nihongo");
        press(fe, uuid, "Return");
        std::cerr << "E2E-OK\n";
        instance.exit();
    });
    instance.exec();
    return failures;
}
