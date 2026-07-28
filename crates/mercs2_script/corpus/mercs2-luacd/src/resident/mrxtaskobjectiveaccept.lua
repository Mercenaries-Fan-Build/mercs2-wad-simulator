inherit("MrxTaskObjectiveAction")
import("MrxGui")
import("MrxPlayer")

function _TargetActioned(self, uActionerGuid, uActioneeGuid)
  if not self._bConfPromptDisplayed then
    self._bConfPromptDisplayed = true
    local sDialogText = MrxUtil.SetDefault(self:GetConfig().sDialogText, "[Generic.Accept]?")
    local uPlayerGuid = Player.GetCharacter(uActionerGuid)
    MrxGui.DisplayDialogBox(uPlayerGuid, sDialogText, {
      "[Generic.Yes]",
      "[Generic.No]"
    }, 1, _ConfPromptDismissed, {
      self,
      uActionerGuid,
      uActioneeGuid
    }, nil, nil, nil, nil, nil, 2)
  end
end

function _ConfPromptDismissed(self, uActionerGuid, uActioneeGuid, nSelectedIndex)
  self._bConfPromptDisplayed = false
  ASSERT(type(nSelectedIndex) == "number")
  if nSelectedIndex == 1 then
    MrxTaskObjectiveAction._TargetActioned(self, uActionerGuid, uActioneeGuid)
  end
end

function _PrintObjectiveMessage(self, sMsgType)
end
