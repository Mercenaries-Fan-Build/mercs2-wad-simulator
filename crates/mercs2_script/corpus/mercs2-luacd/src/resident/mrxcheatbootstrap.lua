import("MrxFactionManager")
import("MrxLayerManager")
import("MrxMultiPageMenu")
import("MrxPlayState")
import("MrxRewardData")
import("MrxTaskState")
import("MrxTransit")
import("MrxUtil")
import("WifMissionData")
import("WifMissionFlow")
import("WifVzBoundary")
import("WifCheatStockpile")
import("MrxPmc")
import("MrxSupportData")
import("WifPmcInterior")
import("Munitions")
_sLastCompletedContractName = "NoContract"

function SetTaskTreeRoot(oCurrTask)
  _oCurrTask = oCurrTask
end

function DisplayOptions()
  _DisplayRootDialog()
end

function _AddRootOption()
  MrxMultiPageMenu.AddOption("Return to root menu", _DisplayRootDialog, nil, true, true)
end

function _AddCloseOption()
  MrxMultiPageMenu.AddOption("Close this menu", nil, nil, true, true)
end

function _DisplayRootDialog()
  MrxMultiPageMenu.Reset()
  if not WifPmcInterior.IsInside() then
    MrxMultiPageMenu.AddOption("Skip to a mission", _DisplaySkipDialog, {false})
    MrxMultiPageMenu.AddOption("Skip to a briefing", _DisplaySkipDialog, {true})
    local oCurrentContract = MrxPlayState.GetCurrentMission()
    if oCurrentContract then
      local oParent = oCurrentContract:GetParent()
      local sParentName = oParent:GetName()
      MrxMultiPageMenu.AddOption("Complete current contract (" .. sParentName .. ")", _CompleteCurrentContract)
    end
    MrxMultiPageMenu.AddOption("Traverse mission hierarchy", _DisplayTraverseDialog)
  end
  MrxMultiPageMenu.AddOption("Add cash", _DisplayAddCashDialog)
  MrxMultiPageMenu.AddOption("Add fuel", _DisplayAddFuelDialog)
  MrxMultiPageMenu.AddOption("Add support", _DisplayAddSupportDialog)
  MrxMultiPageMenu.AddOption("Modify attitude", _DisplayModAttitudeDialog, {
    nil,
    nil,
    nil
  })
  MrxMultiPageMenu.AddOption("Unlock all landing zones", MrxTransit.UnlockAllLandingZones)
  MrxMultiPageMenu.AddOption("Dispense all rewards", MrxRewardData.DispenseAllRewards)
  _AddCloseOption()
  MrxMultiPageMenu.Display("Welcome to the Cheat Menu.")
end

function _CompleteCurrentContract()
  local oCurrentContract = MrxPlayState.GetCurrentMission()
  oCurrentContract:Complete()
end

function _DisplaySkipDialog(bDoBriefing)
  MrxMultiPageMenu.Reset()
  local tMissionNames = {}
  for sMissionName, _ in pairs(WifMissionData.tMissionData) do
    table.insert(tMissionNames, sMissionName)
  end
  table.sort(tMissionNames)
  for _, sMissionName in ipairs(tMissionNames) do
    MrxMultiPageMenu.AddOption(sMissionName, _fMissionSkipDialogCallback, {sMissionName, bDoBriefing})
  end
  _AddRootOption()
  _AddCloseOption()
  if bDoBriefing then
    MrxMultiPageMenu.Display("Select a briefing:")
  else
    MrxMultiPageMenu.Display("Select a mission:")
  end
end

function _DisplayTraverseDialog()
  local oParent = _oCurrTask:GetParent()
  if oParent and _oCurrTask:IsCompleted() then
    _oCurrTask = oParent
    _DisplayTraverseDialog()
  else
    MrxMultiPageMenu.Reset()
    local tTaskNames = {}
    local tChildren = _oCurrTask:GetChildren()
    for sChildName, oChild in pairs(tChildren) do
      if oChild:IsActive() then
        table.insert(tTaskNames, sChildName)
      end
    end
    table.sort(tTaskNames)
    for _, sTaskName in ipairs(tTaskNames) do
      MrxMultiPageMenu.AddOption(sTaskName, function()
        _oCurrTask = _oCurrTask:GetChild(sTaskName)
        _DisplayTraverseDialog()
      end)
    end
    if oParent then
      if _oCurrTask:_CanCompleteViaCheatMenu() then
        MrxMultiPageMenu.AddOption("Complete", function()
          _oCurrTask:Complete()
          _oCurrTask = _oCurrTask:GetParent()
        end)
      end
      local nPlayState = MrxPlayState.Get()
      if nPlayState == MrxPlayState._knMission then
        MrxMultiPageMenu.AddOption("Cancel", function()
          _oCurrTask:Cancel()
          _oCurrTask = _oCurrTask:GetParent()
        end)
      end
      MrxMultiPageMenu.AddOption("Up a level", function()
        _oCurrTask = _oCurrTask:GetParent()
        _DisplayTraverseDialog()
      end)
    end
    _AddRootOption()
    _AddCloseOption()
    MrxMultiPageMenu.Display("You are here:\n" .. _oCurrTask:GetLineage())
  end
end

function _DisplayAddCashDialog()
  MrxMultiPageMenu.Reset()
  for _, nCash in ipairs({
    1000,
    10000,
    100000,
    1000000,
    10000000,
    100000000
  }) do
    MrxMultiPageMenu.AddOption(string.format("+$%d", nCash), function(nCash)
      MrxPmc.AddCashQty(nCash)
      _DisplayAddCashDialog()
    end, {nCash})
  end
  _AddRootOption()
  _AddCloseOption()
  MrxMultiPageMenu.Display([[
Add Cash
($]] .. MrxPmc.GetCashQty() .. ")")
end

function _DisplayAddFuelDialog()
  MrxMultiPageMenu.Reset()
  for _, nFuel in ipairs({
    10,
    100,
    1000,
    9999
  }) do
    MrxMultiPageMenu.AddOption(string.format("+%d", nFuel), function(nFuel)
      if nFuel + MrxPmc.GetFuelQty() > MrxPmc.GetFuelCapacity() then
        MrxPmc.SetFuelCapacity(9999, true)
      end
      MrxPmc.AddFuelQty(nFuel)
      _DisplayAddFuelDialog()
    end, {nFuel})
  end
  _AddRootOption()
  _AddCloseOption()
  MrxMultiPageMenu.Display([[
Add Fuel
(]] .. MrxPmc.GetFuelQty() .. ")")
end

function _DisplayAddSupportDialog()
  local tSupportData = MrxSupportData.tSupportData
  local tSupportKeys = {}
  for sKey, _ in pairs(tSupportData) do
    table.insert(tSupportKeys, sKey)
  end
  table.sort(tSupportKeys, function(a, b)
    return tSupportData[a].sName < tSupportData[b].sName
  end)
  MrxMultiPageMenu.Reset()
  MrxMultiPageMenu.AddOption("The Works! + $ + F", function()
    for sKey, tData in pairs(tSupportData) do
      MrxPmc.AddSupportQty(sKey, tData.nMaxStock - (MrxPmc.GetSupportQty(sKey) or 0))
    end
    MrxPmc.AddCashQty(10000000)
    MrxPmc.SetFuelCapacity(9999, true)
    MrxPmc.AddFuelQty(9999)
    MrxSupportData.SetIgnoreRequirements(true)
  end)
  for _, sKey in ipairs(tSupportKeys) do
    MrxMultiPageMenu.AddOption(string.format("%s (%d/%d)", tSupportData[sKey].sName, MrxPmc.GetSupportQty(sKey) or 0, tSupportData[sKey].nMaxStock), function(sKey)
      MrxPmc.AddSupportQty(sKey, 1)
      _DisplayAddSupportDialog()
    end, {sKey})
  end
  _AddRootOption()
  _AddCloseOption()
  MrxMultiPageMenu.Display("Add Support")
end

function _DisplayModAttitudeDialog(sSubjectAbbrev, sObjectAbbrev, nRelation)
  sSubjectAbbrev = sSubjectAbbrev or _sSubjectAbbrev or "All"
  sObjectAbbrev = sObjectAbbrev or _sObjectAbbrev or "Pmc"
  if nRelation then
    MrxFactionManager.SetRelation(sSubjectAbbrev, sObjectAbbrev, nRelation)
  else
    nRelation = MrxFactionManager.GetRelation(sSubjectAbbrev, sObjectAbbrev)
  end
  _sSubjectAbbrev = sSubjectAbbrev
  _sObjectAbbrev = sObjectAbbrev
  MrxMultiPageMenu.Reset()
  MrxMultiPageMenu.AddOption("Change subject", _DisplayFactionDialog, {true})
  MrxMultiPageMenu.AddOption("Change object", _DisplayFactionDialog, {false})
  MrxMultiPageMenu.AddOption("Change attitude", _DisplayAttitudeDialog)
  _AddRootOption()
  _AddCloseOption()
  local nLevel = MrxFactionManager.ConvertRelationToAttitudeLevel(nRelation)
  local sLabel = MrxFactionManager.GetAttitudeFromLevel(nLevel)
  MrxMultiPageMenu.Display([[
Modify faction attitude
Subject: ]] .. sSubjectAbbrev .. [[

Object: ]] .. sObjectAbbrev .. [[

Attitude: ]] .. sLabel)
end

function _DisplayFactionDialog(bSubject)
  MrxMultiPageMenu.Reset()
  local tFactionAbbrevs = MrxFactionManager.GetFactionAbbrevs()
  for _, sFactionAbbrev in ipairs(tFactionAbbrevs) do
    MrxMultiPageMenu.AddOption(sFactionAbbrev, _DisplayModAttitudeDialog, {
      bSubject and sFactionAbbrev,
      not bSubject and sFactionAbbrev,
      nil
    })
  end
  MrxMultiPageMenu.AddOption("Back", _DisplayModAttitudeDialog, {
    nil,
    nil,
    nil
  }, true)
  _AddRootOption()
  _AddCloseOption()
  MrxMultiPageMenu.Display("Choose faction")
end

function _DisplayAttitudeDialog()
  MrxMultiPageMenu.Reset()
  local tAttitudes = MrxFactionManager.GetAttitudes()
  local tAttitudeList = {}
  for sAttitude, nRelation in pairs(tAttitudes) do
    table.insert(tAttitudeList, {sAttitude = sAttitude, nRelation = nRelation})
  end
  table.sort(tAttitudeList, function(a, b)
    return a.nRelation > b.nRelation
  end)
  for _, tAttitude in ipairs(tAttitudeList) do
    MrxMultiPageMenu.AddOption(string.format("%s (%d)", tAttitude.sAttitude, tAttitude.nRelation), _DisplayModAttitudeDialog, {
      nil,
      nil,
      tAttitude.nRelation
    })
  end
  MrxMultiPageMenu.AddOption("Back", _DisplayModAttitudeDialog, {
    nil,
    nil,
    nil
  }, true)
  _AddRootOption()
  _AddCloseOption()
  MrxMultiPageMenu.Display("Choose attitude")
end

_G.Cheat = {DisplayOptions = DisplayOptions}

function _G.DebugTeleport(x, y, z)
  local tPlayers = Player.GetAllPlayers()
  local tLocs = {}
  for k, v in pairs(tPlayers) do
    table.insert(tLocs, {
      x,
      y,
      z
    })
  end
  MrxUtil.TeleportHeroesToLocations(tLocs)
end

function EnableSkipMode(bEnable, sMissionId, bBriefing)
  if bEnable then
    _sSkipToMissionId = sMissionId
    _bSkipToBriefing = bBriefing
  else
    if _sSkipToMissionId then
      local tExpectedResources = WifCheatStockpile[_sSkipToMissionId]
      if tExpectedResources then
        if tExpectedResources.tSupport then
          for sSupportId, nQuantity in pairs(tExpectedResources.tSupport) do
            MrxPmc.AddSupportQty(sSupportId, nQuantity)
          end
        end
        if tExpectedResources.tEquipment then
          for i, sEquipmentId in ipairs(tExpectedResources.tEquipment) do
            MrxPmc.AddEquipment(sEquipmentId)
          end
        end
        if tExpectedResources.nCash then
          MrxPmc.AddCashQty(MrxPmc.GetCashQty() * -1)
          MrxPmc.AddCashQty(tExpectedResources.nCash)
        end
        if tExpectedResources.nFuel then
          MrxPmc.AddFuelQty(MrxPmc.GetFuelQty() * -1)
          MrxPmc.AddFuelQty(tExpectedResources.nFuel)
        end
      end
    end
    _sSkipToMissionId = nil
    _bSkipToBriefing = nil
  end
end

function IsSkipModeEnabled()
  return _sSkipToMissionId ~= nil
end

function GetMissionSkipData()
  return _sSkipToMissionId, _bSkipToBriefing
end

function SetMissionSkipDialogCallback(fMissionSkipDialogCallback)
  _fMissionSkipDialogCallback = fMissionSkipDialogCallback
end
