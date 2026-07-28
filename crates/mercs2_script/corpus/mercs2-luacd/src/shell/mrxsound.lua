import("MrxMusic")
import("MrxSoundCategories")
import("MrxSoundBanks")

function EnterShellState()
  Sound.SetMasterVolume(1, 0)
  Sound.TransitionMusic("silence")
  MrxSoundBanks._LoadRequiredAssetsCommon()
  MrxSoundBanks.LoadSoundBank("ui_shell", _StartShellMusic)
  MrxSoundBanks.LoadWaveBank("ui_shell", _StartShellMusic)
  MrxSoundBanks.LoadSoundBank("ui_hud", _StartShellMusic)
  MrxSoundBanks.LoadWaveBank("ui_hud", _StartShellMusic)
  MrxSoundBanks.LoadSoundBank("music", _StartShellMusic)
  MrxSoundBanks.LoadWaveBank("music", _StartShellMusic)
end

function ExitShellState()
  MrxSoundBanks.UnloadSoundBank("ui_shell")
  MrxSoundBanks.UnloadWaveBank("ui_shell")
  MrxSoundBanks.UnloadSoundBank("ui_hud")
  MrxSoundBanks.UnloadWaveBank("ui_hud")
  MrxSoundBanks.UnloadSoundBank("music")
  MrxSoundBanks.UnloadWaveBank("music")
  MrxSoundBanks._UnloadRequiredAssetsCommon()
  Sound.TransitionMusic("silence")
  Debug.Printf("Shell exited")
end

function _StartShellMusic()
  Debug.Printf("Shell music started.")
  Sound.TransitionMusic("shell")
end

_bExitingGame = false

function _SetupGameExit()
  _bExitingGame = false
  Event.Create(Event.GameStateChange, {"unloading", "enter"}, ExitGame)
end

function ExitGame()
  Sound.SetMasterVolume(0, 0.5)
  _bExitingGame = true
end

function ExitingGame()
  return _bExitingGame
end

function EnterPauseState()
  Sound.TransitionMusic("pause")
  MrxMusic._DisableDynamicMusic()
end

function ExitPauseState()
  MrxMusic._RestoreDynamicMusic()
  Sound.TransitionMusic("silence")
end

function EnterCinematicState()
  MrxMusic._DisableDynamicMusic()
end

function ExitCinematicState()
  MrxMusic._RestoreDynamicMusic()
end

function EnterPDAState()
  MrxMusic._DisableDynamicMusic()
  Sound.SetTimerUpdateMusic(false)
end

function ExitPDAState()
  MrxMusic._RestoreDynamicMusic()
  Sound.SetTimerUpdateMusic(true)
end

function EnterAttractState()
  Sound.SetDynamicMusic(false)
end

function ExitAttractState()
  Sound.SetDynamicMusic(true)
end

function BeginActionHijack(bUseHijackMusic)
  MrxSoundCategories.Fade("actionhijack", true)
  if bUseHijackMusic and not MrxMusic._IsPlayingSpecialMusic() then
    Sound.TransitionMusic("hijack")
    Sound.LockActionLevelMusic(true)
  end
end

function EndActionHijack(bUseHijackMusic, bSuccess)
  if bUseHijackMusic and not MrxMusic._IsPlayingSpecialMusic() then
    if bSuccess then
      Sound.TransitionMusic("hijack_success")
    else
      Sound.TransitionMusic("action")
    end
    Sound.LockActionLevelMusic(false)
  end
  MrxSoundCategories.Fade("actionhijack", false)
end

_bSurvivalModeStarted = false

function BeginSurvivalMode()
  Sound.SetSurvivalMode(true)
  MrxSoundCategories.Fade("survivalmode", true)
  Sound.CueSound(0, "sfx_survival_lp")
  MrxSoundCategories.Pitch("survivalmode", true)
  _bSurvivalModeStarted = true
end

function EndSurvivalMode()
  if _bSurvivalModeStarted then
    Sound.SetSurvivalMode(false)
    MrxSoundCategories.Fade("survivalmode", false)
    Sound.StopSound(0, "sfx_survival_lp")
    MrxSoundCategories.Pitch("survivalmode", false)
  end
  _bSurvivalModeStarted = false
end

function EnterInterior()
  MrxMusic.Reset()
  Sound.LockActionLevelMusic(true)
  Sound.TransitionMusic("silence")
end

function ExitInterior()
  Sound.LockActionLevelMusic(false)
  Sound.TransitionMusic("explore")
end

function BeginTransit()
  Sound.TransitionMusic("silence")
end

function EndTransit()
  if MrxMusic._IsPlayingSpecialMusic() then
    MrxMusic._ResumeSpecialMusic()
  else
    Sound.TransitionMusic("explore")
  end
end

function EnterSatelliteView()
  MrxSoundCategories.Fade("satelliteview", true)
  Sound.LockListenerPosition(true)
end

function ExitSatelliteView()
  Sound.LockListenerPosition(false)
  MrxSoundCategories.Fade("satelliteview", false)
end

function EnterScopeView()
  Sound.LockListenerPosition(true)
end

function ExitScopeView()
  Sound.LockListenerPosition(false)
end

_bSoundSystemReady = false

function _FlagSystemReady()
  _bSoundSystemReady = true
  _CheckSoundReady()
end

_bWaitForSoundAssets = false
_funcSoundReadyCallback = nil

function SetSoundReadyFunc(funcSoundReady, bWaitForSoundAssets)
  _bWaitForSoundAssets = bWaitForSoundAssets
  _funcSoundReadyCallback = funcSoundReady
  Sound.RegisterReadyCallback(_FlagSystemReady)
  _CheckSoundReady()
end

function _CheckSoundReady()
  local bNoOutstandingAssets = MrxSoundBanks._nOutstandingAssets == 0
  local bAssetsReady = not _bWaitForSoundAssets or _bWaitForSoundAssets and bNoOutstandingAssets
  if _funcSoundReadyCallback and _bSoundSystemReady and bAssetsReady then
    _funcSoundReadyCallback()
    _funcSoundReadyCallback = nil
  end
end

function Initialize()
  MrxMusic._InitializeMusic()
  MrxSoundCategories._AdditionalFadeSetup()
  _SetupGameExit()
end
