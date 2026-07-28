import("MrxFactionManager")
import("MrxHqManager")
import("MrxStarter")
import("MrxSupportData")
import("WifStarterData")
_tStarters = {}

function Init()
  MrxFactionManager.CreatePersistentAttitudeChangeEvent({
    nil,
    "Pmc",
    nil,
    nil
  }, function()
    for sStarterName, oStarter in pairs(_tStarters) do
      oStarter:RefreshBriefingRoomDisplay()
    end
  end)
  MrxSupportData.SetHeliPilotRecruited(false)
  MrxSupportData.SetMechanicRecruited(false)
  MrxSupportData.SetJetPilotRecruited(false)
end

function GetStarter(sName)
  return _tStarters[sName]
end

function GetStarters()
  return _tStarters
end

function RequestStarter(sName, bFanfareDisplayed)
  local oStarter = GetStarter(sName)
  if oStarter then
    return oStarter
  else
    return CreateStarter(sName, bFanfareDisplayed)
  end
end

function CreateStarter(sName, bFanfareDisplayed)
  local tStarterData = WifStarterData[sName]
  tStarterData.sName = sName
  local oStarter = MrxStarter:Create(tStarterData)
  if bFanfareDisplayed then
    oStarter:_SetFanfareDisplayed(bFanfareDisplayed)
  end
  oStarter:Activate()
  _tStarters[sName] = oStarter
  if tStarterData.bPmcStarter then
    if sName == "HelPmcBoss" then
      MrxSupportData.SetHeliPilotRecruited(true)
    elseif sName == "MecPmcBoss" then
      MrxSupportData.SetMechanicRecruited(true)
    elseif sName == "JetPmcBoss" then
      MrxSupportData.SetJetPilotRecruited(true)
    end
  end
  return oStarter
end

function DestroyStarter(sName)
  local oStarter = _tStarters[sName]
  if oStarter then
    oStarter:Deactivate()
    _tStarters[sName] = nil
  end
end

function DestroyAllStarters()
  for sName, oStarter in pairs(_tStarters) do
    oStarter:Deactivate()
  end
  _tStarters = {}
end

function GetStarterIndexFromName(sStarterName)
  local nIndex = 1
  for i, sName in ipairs(WifStarterData._sStarters) do
    if sName == sStarterName then
      return nIndex
    end
    nIndex = nIndex + 1
  end
  Debug.Printf("NET WARNING: could not find index for Starter name " .. tostring(sStarterName))
  return nil
end

function GetStarterNameFromIndex(nStarterIndex)
  local nIndex = 1
  for i, sName in ipairs(WifStarterData._sStarters) do
    if nIndex == nStarterIndex then
      return sName
    end
    nIndex = nIndex + 1
  end
  Debug.Printf("NET WARNING: could not find name for Starter index " .. tostring(nStarterIndex))
  return nil
end

function SaveSingleton()
  local tSaveData = {}
  for sName, oStarter in pairs(_tStarters) do
    local tData = {}
    tData.bFanfareDisplayed = oStarter:HasFanfareBeenDisplayed()
    tData.bCardDisplayed = oStarter:HasCardBeenDisplayed()
    tData.tIntros = oStarter:GetIntros()
    tData.tOldBriefings = oStarter:GetOldBriefings()
    tSaveData[sName] = tData
  end
  return tSaveData
end

function LoadSingleton(tSaveData)
  for sName, tData in pairs(tSaveData) do
    local oStarter = RequestStarter(sName, tData.bFanfareDisplayed)
    if oStarter then
      oStarter:_SetFanfareDisplayed(tData.bFanfareDisplayed)
      oStarter:_SetCardDisplayed(tData.bCardDisplayed)
      if tData.tIntros then
        for sIntroName, bIntroViewed in pairs(tData.tIntros) do
          if bIntroViewed == true then
            oStarter:AddIntro(sIntroName)
            oStarter:SetViewedIntro(sIntroName, bIntroViewed)
          elseif bIntroViewed == false then
            oStarter:AddIntro(sIntroName)
          end
        end
      end
      if tData.tOldBriefings then
        for sMissionName in pairs(tData.tOldBriefings) do
          oStarter:SetBriefingOld(sMissionName)
        end
      end
    end
  end
end
