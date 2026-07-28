import("MrxSound")
import("MrxSoundCategories")
_nOutstandingAssets = 0
_nSubmittedRequests = 0
local MAX_SUBMITTED = 64
_tPendingRequests = {}
_nLastAddedIndex = 0
_funcBatchComplete = nil

function LoadSoundBank(sBank, funcBatchComplete)
  _AddAssetRequest(sBank, "soundbank", true)
  if funcBatchComplete then
    _funcBatchComplete = funcBatchComplete
  end
end

function UnloadSoundBank(sBank, funcBatchComplete)
  _AddAssetRequest(sBank, "soundbank", false)
  if funcBatchComplete then
    _funcBatchComplete = funcBatchComplete
  end
end

function LoadWaveBank(sBank, funcBatchComplete)
  _AddAssetRequest(sBank, "wavebank", true)
  if funcBatchComplete then
    _funcBatchComplete = funcBatchComplete
  end
end

function UnloadWaveBank(sBank, funcBatchComplete)
  _AddAssetRequest(sBank, "wavebank", false)
  if funcBatchComplete then
    _funcBatchComplete = funcBatchComplete
  end
end

function LoadTempBank(sBank, sType, funcCallback, tCallbackData)
  Sound.LoadTempBank(_GetLocalizedName(sBank), sType, funcCallback, tCallbackData)
end

function UnloadTempBank(sBank, sType, funcCallback, tCallbackData)
  Sound.UnloadTempBank(_GetLocalizedName(sBank), sType, funcCallback, tCallbackData)
end

function RequestAmbienceBank(sBank)
  if Sound._GetLibVersion() >= 12 then
    Sound.RequestAmbienceBank(_GetLocalizedName(sBank))
  end
end

function _SubmitAssetRequest()
  if _nSubmittedRequests < MAX_SUBMITTED and _nLastAddedIndex > 0 then
    local bLoad = _tPendingRequests[_nLastAddedIndex][3]
    local sBank = _tPendingRequests[_nLastAddedIndex][1]
    local sType = _tPendingRequests[_nLastAddedIndex][2]
    if bLoad then
      Sound.LoadBankWithCallback(_GetLocalizedName(sBank), sType, _FlagAssetOpComplete)
    else
      Sound.UnloadBankWithCallback(_GetLocalizedName(sBank), sType, _FlagAssetOpComplete)
    end
    _nLastAddedIndex = _nLastAddedIndex - 1
    _nSubmittedRequests = _nSubmittedRequests + 1
    Debug.Printf("Submitted request for " .. _GetLocalizedName(sBank) .. ", " .. sType .. " with " .. tostring(_nSubmittedRequests) .. " submitted, " .. tostring(_nLastAddedIndex) .. " last index, and " .. tostring(_nOutstandingAssets) .. " outstanding")
  end
end

function _AddAssetRequest(sBank, sType, bLoad)
  Debug.Printf("Added request " .. sBank .. ", " .. sType .. ", " .. tostring(bLoad))
  _nLastAddedIndex = _nLastAddedIndex + 1
  _tPendingRequests[_nLastAddedIndex] = {
    sBank,
    sType,
    bLoad
  }
  _nOutstandingAssets = _nOutstandingAssets + 1
  _SubmitAssetRequest()
end

function _GetLocalizedName(sAssetName)
  local sPrefix = string.sub(sAssetName, 1, 3)
  if sPrefix == "vo_" then
    local sLanguage = Gui.GetLanguageName()
    return sAssetName .. "." .. sLanguage
  end
  return sAssetName
end

function _OpenStreamFiles()
  function _StripPWSExtension(sFileName)
    local nExtensionIndex = string.find(sFileName, ".pws")
    
    return string.sub(sFileName, 1, nExtensionIndex - 1)
  end
  
  function _OpenFile(sFileName)
    local sStripped = _StripPWSExtension(sFileName)
    local sLocalized = _GetLocalizedName(sStripped)
    local sAudioDir = Sound.GetAudioDir()
    local sStreamFile = sAudioDir .. "\\" .. sLocalized .. ".pws"
    Debug.Printf("Opening filename " .. sStreamFile .. " with alias " .. sFileName)
    Sound.OpenStreamFile(sStreamFile, sFileName)
  end
  
  _OpenFile("vo_stream.pws")
  _OpenFile("music.pws")
  _OpenFile("ambience.pws")
end

function _LoadRequiredAssetsCommon()
  _OpenStreamFiles()
  Pg.LoadAsset("Mercs2Globals", "sounddb", MrxSoundCategories._DuckGlobalTable)
  Pg.LoadAsset("MusicMarkers", "musicmarkers")
  Pg.LoadAsset("MusicTransitions", "musictransitions")
end

function _UnloadRequiredAssetsCommon()
  Pg.UnloadAsset("Mercs2Globals", "sounddb")
  Pg.UnloadAsset("MusicMarkers", "musicmarkers")
  Pg.UnloadAsset("MusicTransitions", "musictransitions")
end

function _LoadRequiredAssets()
  _LoadRequiredAssetsCommon()
  Pg.LoadAsset("VehicleEngines", "animationtable")
  Pg.LoadAsset("Sounds", "animationtable")
  Pg.LoadAsset("SoundsAppendix", "animationtable")
  Pg.LoadAsset("SoundMatch", "animationtable")
  Pg.LoadAsset("SoundKey", "materialkeytable")
end

function _UnloadRequiredAssets()
  _UnloadRequiredAssetsCommon()
  Pg.UnloadAsset("VehicleEngines", "animationtable")
  Pg.UnloadAsset("Sounds", "animationtable")
  Pg.UnloadAsset("SoundsAppendix", "animationtable")
  Pg.UnloadAsset("SoundMatch", "animationtable")
  Pg.UnloadAsset("SoundKey", "materialkeytable")
end

function _FlagAssetOpComplete()
  Debug.Printf("AssetOpComplete: " .. tostring(_nSubmittedRequests) .. " submitted, " .. tostring(_nOutstandingAssets) .. " oustanding, " .. tostring(_nLastAddedIndex) .. " last added")
  _nSubmittedRequests = _nSubmittedRequests - 1
  _nOutstandingAssets = _nOutstandingAssets - 1
  _SubmitAssetRequest()
  if _nOutstandingAssets == 0 and _funcBatchComplete then
    Debug.Printf("AssetOp Batch Complete")
    _funcBatchComplete()
    _funcBatchComplete = nil
    MrxSound._CheckSoundReady()
  end
end
