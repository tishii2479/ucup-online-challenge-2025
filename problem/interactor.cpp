#include "testlib.h"
#include <bits/stdc++.h>
using namespace std;
typedef long long ll;
const int num_pkt_types = 7, num_special_work = 8, special_node_id = 8,
          MAX_STR_LEN = 10000, NUM_NODES = 20, MAX_BATCH_SIZE = 128,
          NUM_REPEATS = 5, MAX_NUM_PKTS = 100000, MAX_TIME = 10000000,
          MAX_NUM_CORES = 32, U = 1000000, W = 0xFF;
int c_r = 20;
const int POS_START = -1, POS_END = -2;
const double eps = 1e-5;
const int infty = 1000000000;

int c[NUM_NODES + 1][MAX_BATCH_SIZE + 1], bs[NUM_NODES + 1];
double bestc[NUM_NODES + 1];
int bestcBs[NUM_NODES + 1];
int cwork[num_special_work];
int first_special[num_pkt_types + 1];
vector<int> path[num_pkt_types + 1];
int n, num_cores, c_switch;

struct Pkt
{
    int type, arrive, timeout, w;
    int t, departure, pos, core, has_w;
} pkts[MAX_NUM_PKTS + 1];

int core_valid_time[MAX_NUM_CORES + 1];
int cur_pkt_id, prev_action_time, num_completed_pkts;
ll best_score;

void Send(int x)
{
    printf("%d\n", x);
    fflush(stdout);
}

void Fail(string s = "Invalid Action")
{
    Send(-1);
    tout << s << endl;
    quitf(_wa, "%s\n", s.c_str());
}

void ReadInput()
{
    for (int i = 1; i <= num_pkt_types; ++i)
    {
        int len = inf.readInt();
        path[i].resize(len);
        first_special[i] = -1;
        for (int j = 0; j < len; ++j)
        {
            int &v = path[i][j];
            v = inf.readInt();
            if (v == special_node_id && first_special[i] == -1)
                first_special[i] = j;
        }
    }

    for (int i = 1; i <= NUM_NODES; ++i)
    {
        bs[i] = inf.readInt();
        for (int j = 1; j <= bs[i]; ++j)
            c[i][j] = inf.readInt();
    }

    for (int i = 0; i < num_special_work; ++i)
        cwork[i] = inf.readInt();

    num_cores = inf.readInt();
    c_switch = inf.readInt();
    int _c_r = inf.readInt();
    if (_c_r != c_r)
        Fail("Invalid Input");
}

void PrintInput()
{
    for (int i = 1; i <= num_pkt_types; ++i)
    {
        int len = path[i].size();
        printf("%d ", len);
        for (int j = 0; j < len; ++j)
            printf("%d%c", path[i][j], j + 1 == len ? '\n' : ' ');
    }

    for (int i = 1; i <= NUM_NODES; ++i)
    {
        printf("%d ", bs[i]);
        for (int j = 1; j <= bs[i]; ++j)
            printf("%d%c", c[i][j], j == bs[i] ? '\n' : ' ');
    }

    for (int i = 0; i < num_special_work; ++i)
        printf("%d%c", cwork[i], i + 1 == num_special_work ? '\n' : ' ');

    printf("%d %d %d\n", num_cores, c_switch, c_r);
    fflush(stdout);
}

void Init()
{
    for (int i = 0; i <= num_cores; ++i)
        core_valid_time[i] = 1;
    cur_pkt_id = 1;
    prev_action_time = 1;
    num_completed_pkts = 0;
}

void ReadSample()
{
    n = inf.readInt();
    for (int i = 1; i <= n; ++i)
    {
        int id = inf.readInt();
        pkts[i].arrive = inf.readInt();
        pkts[i].type = inf.readInt();
        pkts[i].timeout = inf.readInt();
        pkts[i].w = inf.readInt();

        pkts[i].t = 0;
        pkts[i].departure = 0;
        pkts[i].pos = POS_START;
        pkts[i].core = 0;
        pkts[i].has_w = 0;
    }
}

int GetSpecialWorkCost(int w)
{
    int ans = 0;
    for (int i = 0; i < num_special_work; ++i)
        if ((1 << i) & w)
            ans += cwork[i];
    return ans;
}

void Run()
{
    // tout<<"Run"<<endl;
    int id[MAX_BATCH_SIZE + 1];
    Send(n);
    while (1)
    {
        string op = ouf.readToken("R|E|Q|F");
        int t, coreId, nodeId, s;
        if (op == "R")
        {
            t = ouf.readInt();
            // tout<<"R "<<t<<endl;
            if (t < 1 || t > MAX_TIME || t < prev_action_time)
                Fail("[1] Invalid Action: time " + to_string(t));
            prev_action_time = t;
            if (t < core_valid_time[0])
                Fail("[2] Invalid Action: time " + to_string(t));
            core_valid_time[0] = t + c_r;

            // send pkts
            int num_send_pkts = 0;
            while (cur_pkt_id <= n && pkts[cur_pkt_id].arrive <= t)
            {
                ++num_send_pkts;
                ++cur_pkt_id;
            }
            Send(num_send_pkts);
            for (int i = 0; i < num_send_pkts; ++i)
            {
                int idx = cur_pkt_id - num_send_pkts + i;
                pkts[idx].t = t + c_r;
                pkts[idx].pos = 0;
                pkts[idx].core = 0;
                printf("%d %d %d %d\n", idx, pkts[idx].arrive, pkts[idx].type,
                       pkts[idx].timeout);
            }
            fflush(stdout);
        }
        else if (op == "E")
        {
            t = ouf.readInt();
            // tout<<"E "<<t;
            if (t < 1 || t > MAX_TIME || t < prev_action_time)
                Fail("[3] Invalid Action: time " + to_string(t));
            prev_action_time = t;
            coreId = ouf.readInt();
            // tout<<" "<<coreId;
            if (coreId < 1 || coreId > num_cores)
                Fail("Invalid Action: incorrect coreId");
            nodeId = ouf.readInt();
            // tout<<" "<<nodeId;
            if (nodeId < 1 || nodeId > NUM_NODES)
                Fail("Invalid Action: incorrect nodeId");
            s = ouf.readInt();
            // tout<<" "<<s;
            if (s < 1 || s > bs[nodeId])
                Fail("Invalid Action: incorrect batchsize: " + to_string(s) + " b=" + to_string(bs[nodeId]));
            for (int i = 1; i <= s; ++i)
            {
                id[i] = ouf.readInt();
                // tout<<" "<<id[i];
            }
            // tout<<endl;

            if (t < core_valid_time[coreId])
                Fail("Invalid Action: incorrect time on coreId " +
                     to_string(coreId) + ": t=" + to_string(t) +
                     " available=" + to_string(core_valid_time[coreId]) +
                     " nodeId=" + to_string(nodeId) + " s=" + to_string(s));
            for (int i = 1; i <= s; ++i)
            {
                int idx = id[i];
                if (idx < 1 || idx > n)
                    Fail("Invalid Action: incorrect packet idx");
            }

            int endTime = t + c[nodeId][s];
            for (int i = 1; i <= s; ++i)
            {
                if (pkts[id[i]].core != coreId)
                {
                    endTime += c_switch;
                }
                pkts[id[i]].core = coreId;
            }
            if (nodeId == special_node_id)
            {
                for (int i = 1; i <= s; ++i)
                {
                    // if (!pkts[i].hasw)
                    endTime += GetSpecialWorkCost(pkts[id[i]].w);
                }
            }
            core_valid_time[coreId] = endTime;
            for (int i = 1; i <= s; ++i)
            {
                int idx = id[i];
                int pos = pkts[idx].pos;
                int type = pkts[idx].type;
                if (pos < 0 || pos >= (int)path[type].size())
                    Fail("[1] Invalid Action: illegal nodeId for packet " +
                         to_string(idx) + ", pos=" + to_string(pos));
                int curNode = path[type][pos];
                if (curNode != nodeId)
                    Fail("[2] Invalid Action: illegal nodeId for packet " +
                         to_string(idx) + " curNode=" + to_string(curNode) +
                         " nodeId=" + to_string(nodeId) + " pos=" + to_string(pos) + " type=" + to_string(type));
                if (t < pkts[idx].t)
                    Fail("Invalid Action: packet " + to_string(idx) +
                         " not ready, " + to_string(t) + " < " +
                         to_string(pkts[idx].t));

                pkts[idx].t = endTime;
                ++pkts[idx].pos;
                if (pkts[idx].pos == (int)path[type].size())
                {
                    // finish processing packet idx
                    pkts[idx].departure = endTime;
                    pkts[idx].pos = POS_END;
                    ++num_completed_pkts;
                }
            }
        }
        else if (op == "Q")
        {
            t = ouf.readInt();
            // tout<<"Q "<<t;
            if (t < 1 || t > MAX_TIME || t < prev_action_time)
                Fail("[4] Invalid Action: time " + to_string(t));
            prev_action_time = t;
            int idx = ouf.readInt();
            // tout<<" "<<idx<<endl;
            if (idx < 1 || idx > n)
                Fail();
            if (t < pkts[idx].t)
                Fail();
            if (pkts[idx].has_w)
                Fail("Invalid Action: query special works multiple times");
            pkts[idx].has_w = 1;
            // tout<<"* w="<<pkts[idx].w<<endl;
            Send(pkts[idx].w);
        }
        else if (op == "F")
        {
            // tout<<"F"<<endl;
            //  success
            if (num_completed_pkts != n)
            {
                Fail("Invalid Action: Finish too early");
            }
            return;
        }
        else
        {
            Fail("Invalid Action: incorrect action type");
        }
    }
}

void ComputeScore()
{
    for (int i = 1; i <= n; ++i)
    {
        if (pkts[i].pos != POS_END)
            Fail("Not all packets have been processed");
    }

    int numTimeout = 0;
    for (int i = 1; i <= n; ++i)
    {
        int duration = pkts[i].departure - pkts[i].arrive;
        if (duration <= 0)
            Fail();
        numTimeout += duration > pkts[i].timeout;
        // tout<<pkts[i].arrive<<" "<<pkts[i].departure<<"
        // "<<pkts[i].timeout<<endl;
    }

    int min_arrive = infty, max_departure = 0;
    for (int i = 1; i <= n; ++i)
    {
        min_arrive = min(min_arrive, pkts[i].arrive);
        max_departure = max(max_departure, pkts[i].departure);
    }

    double throughput = 1e6 * (n - 1) / (max_departure - min_arrive);
    throughput /= num_cores;
    double timeout_rate = 1. * numTimeout / n;
    ll score = max(0, (int)((throughput - 1e4 * timeout_rate) * 100));
    best_score = max(best_score, score);
    tout << throughput << " " << timeout_rate << " " << score << endl;
}

void PrintScore() { tout << best_score << endl; }

void RunAll()
{
    for (int I = 0; I < NUM_REPEATS; ++I)
    {
        Init();
        ReadSample();
        Run();
        ComputeScore();
    }
}

int main(int argc, char *argv[])
{
    registerInteraction(argc, argv);
    // FILE *fdbg=fopen("2.dbg","w");

    ReadInput();
    PrintInput();
    RunAll();
    PrintScore();
    quitp(static_cast<int>(best_score));

    return 0;
}
