import("MrxSoundBootstrap")
import("MrxFactionManager")
import("MrxGuiBootstrap")
import("MrxLayerManager")
import("MrxSupportData")
import("MrxPlayer")
import("MrxPmc")
import("MrxState")
import("MrxUtil")
_bGuiLoaded = false
_bLocalPlayerJoined = false
_bHandleStateTransitions = true

function Start(fCallback, tCallbackArgs)
  _bGuiLoaded = false
  _sHeroSpawnLocation = nil
  _bLocalPlayerJoined = false
  _fOnDoneCallback = fCallback
  _tOnDoneCallbackArgs = tCallbackArgs
  MrxGuiBootstrap.SetOnGuiLoadedFunc(_GuiLoaded)
  MrxPlayer.SetLocalPlayerJoinedCallback(_LocalPlayerJoined)
  MrxPlayer.Start()
end

function IsGuiLoaded()
  return _bGuiLoaded
end

function _GuiLoaded()
  Debug.Printf("gui loaded")
  if not _bGuiLoaded then
    _bGuiLoaded = true
    if _bHandleStateTransitions then
      MrxState.Enter(MrxState.STATE_WAITFORGAME, _End)
    else
      _End()
    end
  end
end

function _LocalPlayerJoined()
  Debug.Printf("local player joined")
  if not _bLocalPlayerJoined then
    _bLocalPlayerJoined = true
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
  if _bHandleStateTransitions then
    MrxState.Exit(MrxState.STATE_WAITFORGAME)
  end
  MrxFactionManager.Setup()
  local sLevelName = string.lower(Sys.GetLevelName())
  if sLevelName ~= "vz" then
    SetDefaultAtmosphere()
  end
  if Sys.StartWithResources and Sys.StartWithResources() then
    MrxPmc.AddCashQty(10000000)
    MrxPmc.SetFuelCapacity(9999, true)
    MrxPmc.AddFuelQty(9999)
    local tSupportData = MrxSupportData.tSupportData
    for sKey, tData in pairs(tSupportData) do
      MrxPmc.AddSupportQty(sKey, tData.nMaxStock - (MrxPmc.GetSupportQty(sKey) or 0))
    end
    MrxSupportData.SetIgnoreRequirements(true)
  end
  MrxUtil.CallWithOptionalArgs(_fOnDoneCallback, _tOnDoneCallbackArgs)
end

function SetDefaultAtmosphere()
  Debug.Printf("Atmosphere: default (non-VZ) settings")
  Graphics.Atmosphere.Begin()
  Graphics.Atmosphere.SetTime(0.3)
  Graphics.Atmosphere.SetSky("afternoon")
  Graphics.Atmosphere.SetTimeSpeed(0)
  Graphics.Atmosphere.SetAmbientCube(0.42, 0.44, 0.49, 0.4, 0.48, 0.47, 0.6, 0.67, 0.68, 0.31, 0.27, 0.12, 0.3, 0.35, 0.4, 0.45, 0.47, 0.37)
  Graphics.Atmosphere.SetAmbientColor(0.45, 0.45, 0.45)
  Graphics.Atmosphere.SetLightIntensity(1)
  Graphics.Atmosphere.SetInscatteringMultiplier(50)
  Graphics.Atmosphere.SetExtinctionMultiplier(0.8)
  Graphics.Atmosphere.SetBetaRayMultiplier(0.001)
  Graphics.Atmosphere.SetBetaMieMultiplier(0.01)
  Graphics.Atmosphere.SetHenyeyGreensteinConst(0.9)
  Graphics.Bloom.SetBlurRadius(0.5)
  Graphics.Bloom.SetThreshold(0.775)
  Graphics.Bloom.SetMultiplier(0)
  Graphics.Monochrome.SetGradient(0, 128, 0, 0, 0, 0, 0, 0.65, 0.35, 0)
  Graphics.Monochrome.SetGradient(128, 255, 0, 0.65, 0.35, 0, 1, 1, 1, 0)
  Graphics.Contrast.SetLimit(0.1)
  Graphics.Contrast.SetMultiplier(1.5)
  Graphics.Atmosphere.End(8)
end

function SetHeroSpawnLocation(sHeroSpawnLocation)
  _sHeroSpawnLocation = sHeroSpawnLocation
end

function SetHandleStateTransitions(bEnable)
  _bHandleStateTransitions = bEnable
end
