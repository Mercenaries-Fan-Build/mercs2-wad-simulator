inherit("MrxTask")
import("MrxSubtitle")
import("MrxVoSequence")
import("WifMissionData")
import("WifMissionFlow")
import("MrxTaskObjective")
import("MrxFactionManager")
import("MrxRewardData")
_knContract = 0
_knJob = 1

function Activated(self)
  MrxTask.Activated(self)
  Graphics.InitTinyGeometry()
  self._tVo = {}
end

function Cleanup(self)
  local tConfig = self:GetConfig()
  if tConfig.oStarter then
    tConfig.oStarter:RemoveMission(self)
  end
  if self._tVo then
    for i, tVo in ipairs(self._tVo) do
      VO.Cancel(tVo.vSpeaker, tVo.sCueHandle)
    end
  end
  MrxSubtitle.ClearPending()
  MrxTask.Cleanup(self)
end

function _PlayVo(self, vSpeaker, sCueHandle, fCallback, tCallbackArgs)
  local bSuccess = VO.Cue(vSpeaker, sCueHandle, fCallback, tCallbackArgs)
  table.insert(self._tVo, {vSpeaker = vSpeaker, sCueHandle = sCueHandle})
  return bSuccess
end

function RefreshPdaDisplay(self)
  local sMissionName = self:GetMissionId()
  local tObjectives = {}
  local numObjectives = 0
  
  local function _AppendDescriptions(oParent)
    local tChildren = oParent:GetChildren()
    for nIndex, oChild in pairs(tChildren) do
      if oChild.GetDisplayDescription and oChild:GetDisplayDescription() and not oChild:IsCompleted() and not oChild:IsCancelled() and oChild.GetDescription and oChild.RefreshPdaDisplay then
        local sDesc = oChild:GetDescription(true)
        if sDesc then
          numObjectives = numObjectives + 1
          tObjectives[numObjectives] = {}
          tObjectives[numObjectives][1] = oChild:GetDescription()
          tObjectives[numObjectives][2] = oChild:GetInlineIcon()
        end
        oChild:RefreshPdaDisplay()
      end
      _AppendDescriptions(oChild)
    end
  end
  
  _AppendDescriptions(self)
  WifMissionFlow.AddPdaMissionDetails(sMissionName, tObjectives)
end

function IsContract()
  return false
end

function IsJob()
  return false
end

function GetNumCompletions(self)
  return WifMissionFlow.GetKeyValue(self:GetMissionId()) or 0
end

function GetMissionId(self)
  return self:GetParent():GetName()
end

function GetFactionId(self)
  return self:GetConfig().sFactionId
end

function GetStartLocations(self)
  return WifMissionFlow.GetMissionStartLocations(self:GetMissionId())
end
