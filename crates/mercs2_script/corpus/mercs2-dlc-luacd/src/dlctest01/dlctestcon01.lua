inherit("MrxTaskContract", false)
import("MrxAchievements", false)

function Activated(self)
  MrxTaskContract.Activated(self)
  Debug.Printf("@@@@@@@@@@ DlcTestCon01: HELLO")
  self:CreateChild({
    sName = "DlcTestCon01",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = Pg.GetGuidByName("Van (Racing) 0x0012b978"),
    sDspShortDesc = "Destroy the van in town",
    tOnComplete = {
      {
        self.Complete,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
end

function Complete(self)
  MrxAchievements.NetGrantAchievement("ACHIEVEMENT_DLC_1_COMPLETE")
  MrxTaskContract.Complete(self)
end
