import("MrxSoundShellBootstrap")
import("MrxGuiBootstrap_ShellOnly")
_bGuiLoaded = false
_bLocalPlayerJoined = false

function Start(fCallback, tCallbackArgs)
  _bGuiLoaded = false
  _bLocalPlayerJoined = false
  _sHeroSpawnLocation = nil
  _fOnDoneCallback = fCallback
  _tOnDoneCallbackArgs = tCallbackArgs
  MrxGuiBootstrap_ShellOnly.SetOnGuiLoadedFunc(_GuiLoaded)
end

function IsGuiLoaded()
  return _bGuiLoaded
end

function _GuiLoaded()
  Debug.Printf("gui loaded")
  if not _bGuiLoaded then
    _bGuiLoaded = true
    _End()
  end
end

function _End()
  if not _bLocalPlayerJoined then
    return
  end
  if not _bGuiLoaded then
    return
  end
end

function SetHeroSpawnLocation(sHeroSpawnLocation)
  _sHeroSpawnLocation = sHeroSpawnLocation
end
