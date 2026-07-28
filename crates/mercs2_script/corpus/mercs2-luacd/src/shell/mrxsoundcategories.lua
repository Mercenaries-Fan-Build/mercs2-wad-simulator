_tFadeSettings = {
  vosequence = {},
  credits = {},
  actionhijack = {},
  survivalmode = {},
  fanfare = {},
  satelliteview = {}
}

function SetFadeCategory(sMode, sCategory, fLevel, fEnterLength, fExitLength)
  _tFadeSettings[sMode][sCategory] = {
    fLevel,
    fEnterLength,
    fExitLength
  }
end

function Fade(sMode, bDown)
  for category, tSettings in pairs(_tFadeSettings[sMode]) do
    local fLevel = tSettings[1]
    local fEnterLength = tSettings[2]
    local fExitLength = tSettings[3]
    if bDown then
      Sound.FadeCategoryDown(category, fLevel, fEnterLength)
    else
      Sound.FadeCategoryUp(category, fExitLength)
    end
  end
end

function _AdditionalFadeSetup()
  SetFadeCategory("credits", "sfx", 0, 0.5, 0.5)
  SetFadeCategory("credits", "vo", 0, 0.5, 0.5)
end

_tPitchSettings = {
  survivalmode = {}
}

function SetPitchCategory(sMode, sCategory, fLevel, fEnterLength, fExitLength)
  _tPitchSettings[sMode][sCategory] = {
    fLevel,
    fEnterLength,
    fExitLength
  }
end

function Pitch(sMode, bDown)
  for category, tSettings in pairs(_tPitchSettings[sMode]) do
    local fLevel = tSettings[1]
    local fEnterLength = tSettings[2]
    local fExitLength = tSettings[3]
    if bDown then
      Sound.PitchCategoryActivate(category, fLevel, fEnterLength)
    else
      Sound.PitchCategoryDeactivate(category, fExitLength)
    end
  end
end

_bDuckOnGlobalTableLoad = false

function SetDuckOnGlobalTableLoad(bDuck)
  _bDuckOnGlobalTableLoad = bDuck
end

function _DuckGlobalTable()
  if _bDuckOnGlobalTableLoad then
    Sound.SetMasterVolume(0, 0.3)
  end
end

_nMasterVolumeRefCount = 0

function DuckMasterVolume(fLength)
  if _nMasterVolumeRefCount == 0 then
    Sound.SetMasterVolume(0, fLength)
  end
  _nMasterVolumeRefCount = _nMasterVolumeRefCount + 1
end

function UnduckMasterVolume(fLength)
  _nMasterVolumeRefCount = _nMasterVolumeRefCount - 1
  if _nMasterVolumeRefCount == 0 then
    Sound.SetMasterVolume(1, fLength)
  end
end
