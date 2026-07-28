import("MrxUtil")
import("MrxVoSequence")
import("MrxPmc")
tEvents = tEvents or {}

function OnActivate(uGuid)
  Debug.Printf("Goal.OnActivate")
  tEvents[uGuid] = tEvents[uGuid] or {}
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, function()
    SetupGoal(uGuid)
  end)
end

function OnDeactivate(uGuid)
  Debug.Printf("Goal.DeActivate")
  tEvents = tEvents or {}
  tEvents[uGuid] = tEvents[uGuid] or {}
  if tEvents[uGuid].GoalVO then
    Event.Delete(tEvents[uGuid].GoalVO)
    tEvents[uGuid].GoalVO = nil
  end
  tEvents[uGuid] = nil
end

function SetupGoal(uBallGuid)
  Debug.Printf("Goal.SetupGoal")
  if Pg.GetGuidByName("LR_Goal") then
    tEvents[uBallGuid].GoalVO = Event.Create(Event.Boundary, {
      uBallGuid,
      Pg.GetGuidByName("LR_Goal"),
      "enter"
    }, function()
      if Pg.GetGuidByName("_global_soccergoal 0x000b0982") then
        MrxVoSequence.Start({
          "Fiona.va3fio12"
        })
        MrxPmc.AddCashQty(100000)
        Object.Remove(Pg.GetGuidByName("LR_Goal"))
      end
    end)
  end
end
